use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::{Stream, StreamExt, future::Either};
use log::warn;
use reqwest::Url;
use tokio::sync::mpsc;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::Status;
use lychee_lib::archive::Archive;
use lychee_lib::async_lib::stream::StreamExt as _;
use lychee_lib::async_lib::waiter::{WaitGroup, WaitGuard};
use lychee_lib::ratelimit::HostPool;
use lychee_lib::{Client, ErrorKind, Request, Response};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::CommandParams;
use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::progress::Progress;
use crate::{ExitCode, cache::Cache};

#[allow(clippy::match_bool, reason = "more readable and compact")]
#[allow(
    clippy::result_large_err,
    reason = "no point in using Box<Response> inside Err when we have whole streams \
              of Response in other places. also, streams are lazy and on-demand."
)]
pub(crate) async fn check(
    params: CommandParams<impl Stream<Item = Result<Request, RequestError>>>,
) -> Result<(ResponseStats, Cache, ExitCode, Arc<HostPool>), ErrorKind> {
    let CommandParams {
        client,
        cache,
        requests,
        cfg,
        is_stdin_input,
    } = params;

    /**** Config options, progress bar, and stats ****/

    let max_concurrency = cfg.max_concurrency();

    let level = cfg.verbose().log_level();
    let hide_bar = cfg.no_progress || is_stdin_input;

    let accept = cfg.accept().into();
    let cache_exclude_status = cfg.cache_exclude_status().into();

    let progress = Progress::new("Extracting links", hide_bar, level, &cfg.mode());

    let mut stats = match cfg.verbose().log_level() >= log::Level::Info {
        true => ResponseStats::extended(),
        false => ResponseStats::default(),
    };

    /*** Input streams and channels (both initial and recursive) ****/

    let (waiter, wait_guard) = WaitGroup::new();

    // split initial requests into three categories by chaining two `partition_result` calls:
    // valid requests, non-fatal request-building errors (reported as normal responses),
    // and fatal errors that abort the run. note that this stream closure *owns* a wait guard,
    // so we must drop the closure after it's finished to avoid deadlock. this is done using
    // the `.chain()` combinator.
    //
    // both halves of each `partition_result` call must be polled concurrently or the pipeline
    // deadlocks (see `PartitionedStream`). `valid_initial_requests` and `early_errors` are both
    // consumed below; `request_error_responses` flows into `combined_responses` via
    // `select_with_strategy` and `fatal_errors` is polled in the final `select`.
    let (valid_initial_requests, early_errors) = requests
        .inspect(|_| progress.inc_length(1))
        .map(move |request| (request, wait_guard.clone()))
        .chain(futures::stream::empty())
        .map(
            |(request, guard)| match request.map_err(RequestError::into_response) {
                Ok(request) => Ok((guard, request)),
                Err(Ok(req_error_resp)) => Err(Ok((guard, req_error_resp))),
                Err(Err(fatal_error)) => Err(Err((guard, fatal_error))),
            },
        )
        .partition_result::<(WaitGuard, Request), Result<_, _>>();

    let (request_error_responses, mut fatal_errors) =
        early_errors.partition_result::<(WaitGuard, Response), (WaitGuard, ErrorKind)>();

    let (recursive_channel_send, recursive_channel_recv) =
        mpsc::unbounded_channel::<(WaitGuard, Request)>();

    let send_recursive_req = |(guard, req)| {
        progress.inc_length(1);
        recursive_channel_send
            .send((guard, req))
            .unwrap_or_else(|e| warn!("unable to send recursive uri {:?} - channel closed?", e.0));
    };

    // combine recursive requests and input requests.
    let combined_requests = futures::stream::select_with_strategy(
        valid_initial_requests,
        UnboundedReceiverStream::new(recursive_channel_recv).take_until(waiter.wait()),
        |()| futures::stream::PollNext::Right, // prefer requests from recursive channel
    );

    /*** Main link checking pipeline ****/

    // perform requests. this is the only part of the main pipeline that happens concurrently.
    let responses = combined_requests
        .map(async |(guard, request)| -> (WaitGuard, Response) {
            let check_url = |r| check_url(&client, r);
            let response = cache
                .handle(&client, &cache_exclude_status, &accept, request, check_url)
                .await;
            (guard, response)
        })
        .buffer_unordered(max_concurrency);

    let combined_responses =
        futures::stream::select_with_strategy(responses, request_error_responses, |()| {
            futures::stream::PollNext::Right
        });
    // increment stats and extract recursive uris from responses.
    let recursive_uris = combined_responses.map(|(guard, response)| -> Vec<(WaitGuard, Request)> {
        progress.update(Some(response.body()));
        stats.add(response);

        let recursive_uris = vec![]; // currently unused.
        let _ = guard;

        recursive_uris
    });

    // send recursive uris back to the initial channel.
    let all_done =
        recursive_uris.for_each(async |uris| uris.into_iter().for_each(send_recursive_req));

    let start = std::time::Instant::now();
    {
        // this `await` is where execution begins. all streams start running and
        // we wait for `all_done` or an early return with an error value.
        match futures::future::select(std::pin::pin!(all_done), fatal_errors.next()).await {
            Either::Left(((), _fatal_errors)) => (),
            Either::Right((None, remaining)) => remaining.await,
            Either::Right((Some((_guard, fatal_error)), _remaining)) => {
                progress.finish("Error while fetching initial inputs");
                return Err(fatal_error);
            }
        }
    }

    /*** Finalising stats and archive suggestion ****/

    progress.finish("Finished processing links");
    stats.duration = start.elapsed();

    if cfg.suggest {
        let progress = Progress::new("Searching for alternatives", hide_bar, level, &cfg.mode());
        let archive = cfg.archive();
        let timeout = cfg.timeout();
        suggest_archived_links(archive, &mut stats, progress, max_concurrency, timeout).await;
    }

    let is_success = match cfg.accept_timeouts {
        true => stats.is_success_ignoring_timeouts(),
        false => stats.is_success(),
    };
    let code = match is_success {
        true => ExitCode::Success,
        false => ExitCode::LinkCheckFailure,
    };
    Ok((stats, cache, code, client.host_pool()))
}

async fn suggest_archived_links(
    archive: Archive,
    stats: &mut ResponseStats,
    progress: Progress,
    max_concurrency: usize,
    timeout: Duration,
) {
    let failed_urls = &get_failed_urls(stats);
    progress.set_length(failed_urls.len() as u64);

    let suggestions = Mutex::new(&mut stats.suggestion_map);

    futures::stream::iter(failed_urls)
        .map(|(input, url)| (input, url, archive.get_archive_snapshot(url, timeout)))
        .for_each_concurrent(max_concurrency, |(input, url, future)| async {
            if let Ok(Some(suggestion)) = future.await {
                suggestions
                    .lock()
                    .unwrap()
                    .entry(input.clone())
                    .or_default()
                    .insert(Suggestion {
                        suggestion,
                        original: url.clone(),
                    });
            }

            progress.update(None);
        })
        .await;

    progress.finish("Finished searching for alternatives");
}

/// Check a URL and return a response.
///
/// # Errors
///
/// This can fail when the URL could not be parsed to a URI.
async fn check_url(client: &Client, request: Request) -> Response {
    // Request was not cached; run a normal check
    let uri = request.uri.clone();
    let source = request.source.clone();
    let span = request.span;
    client.check(request).await.unwrap_or_else(|e| {
        log::error!("Error checking URL {uri}: {e}");
        Response::new(
            uri.clone(),
            Status::Error(ErrorKind::InvalidURI(uri.clone())),
            source.into(),
            span,
            None,
        )
    })
}

fn get_failed_urls(stats: &mut ResponseStats) -> Vec<(InputSource, Url)> {
    stats
        .error_map
        .iter()
        .flat_map(|(source, set)| set.iter().map(move |body| (source, &body.uri)))
        .filter_map(|(source, uri)| {
            if uri.is_data() || uri.is_mail() || uri.is_file() {
                None
            } else {
                match Url::try_from(uri.as_str()) {
                    Ok(url) => Some((source.clone(), url)),
                    Err(_) => None,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_remaps;
    use lychee_lib::{ClientBuilder, ErrorKind, StatusCodeSelector, Uri};

    #[tokio::test]
    async fn test_invalid_url() {
        let client = ClientBuilder::builder().build().client().unwrap();
        let uri = Uri::try_from("http://\"").unwrap();
        let response = client.check_website(&uri, None).await.unwrap();
        assert!(matches!(
            response,
            Status::Unsupported(ErrorKind::BuildRequestClient(_))
        ));
    }

    #[tokio::test]
    async fn test_cache_uses_remapped_uri_as_key() {
        let remaps =
            parse_remaps(&["https://wikipedia.org/ https://wikipedia.org/404".to_string()])
                .unwrap();
        let client = ClientBuilder::builder()
            .remaps(remaps)
            .build()
            .client()
            .unwrap();
        let cache = Cache::new();
        let request = Request::try_from("https://wikipedia.org/").unwrap();
        let response = cache
            .handle(
                &client,
                &StatusCodeSelector::empty().into(),
                &StatusCodeSelector::default_accepted().into(),
                request,
                |r| check_url(&client, r),
            )
            .await;
        assert!(response.status().is_error());
        assert!(cache.contains_key(&Uri::try_from("https://wikipedia.org/404").unwrap()));
    }
}
