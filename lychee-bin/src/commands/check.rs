use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::stream;
use futures::{Stream, StreamExt, future::Either};
use log::warn;
use reqwest::Url;
use tokio::sync::mpsc;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::Status;
use lychee_lib::archive::Archive;
use lychee_lib::async_lib::stream::StreamExt as _;
use lychee_lib::ratelimit::HostPool;
use lychee_lib::{Client, ErrorKind, Request, Response};
use tokio_stream::wrappers::ReceiverStream;

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

    /* Config options, progress bar, and stats */

    let max_concurrency = cfg.max_concurrency().get();

    let level = cfg.verbose().log_level();
    let hide_bar = cfg.no_progress() || is_stdin_input;

    let accept = cfg.accept().into();
    let cache_exclude_status = cfg.cache_exclude_status().into();

    let progress = Progress::new("Extracting links", hide_bar, level, &cfg.mode());

    let mut stats = match cfg.verbose().log_level() >= log::Level::Info {
        true => ResponseStats::extended(),
        false => ResponseStats::default(),
    };

    /* Input streams and channels (both initial and recursive) */

    let (recursive_channel_send, recursive_channel_recv) = mpsc::channel(max_concurrency);
    let recursive_channel_send = RequestQueue(recursive_channel_send);

    // Split initial requests into: valid requests and request errors. Note that
    // this stream closure *owns* the initial queue handle, so we must drop the closure after
    // it's finished to avoid deadlock. This is done using the `.chain()` combinator.
    let (valid_requests, request_errors) = requests
        .inspect(|_| progress.inc_length(1))
        .map(move |request| (request, recursive_channel_send.clone()))
        .chain(futures::stream::empty())
        .map(|(request, queue)| match request {
            Ok(request) => Ok((queue, request)),
            Err(request_error) => Err((queue, request_error)),
        })
        .partition_result::<(RequestQueue, Request), (RequestQueue, RequestError)>();

    // Further partition the request errors into request building errors (like
    // unresolved relative URLs) and fatal errors when fetching a user input fails.
    let (request_building_errors, mut fatal_errors) = request_errors
        .map(
            |(queue, request_error)| match request_error.into_response() {
                Ok(request_building_error) => Ok((queue, request_building_error)),
                Err(fatal_user_input_error) => Err((queue, fatal_user_input_error)),
            },
        )
        .partition_result::<(RequestQueue, Response), (RequestQueue, ErrorKind)>();

    // Combine recursive requests and input requests. The recursive stream ends
    // once every `RequestQueue` handle has been dropped, closing the channel.
    let requests = futures::stream::select_with_strategy(
        valid_requests,
        ReceiverStream::new(recursive_channel_recv),
        |()| futures::stream::PollNext::Right, // Recursive requests consume memory, prefer those.
    );

    /* Main link checking pipeline */

    // Perform requests. This is the only part of the main pipeline that happens concurrently.
    let check_responses = requests
        .map(async |(queue, request)| -> (RequestQueue, Response) {
            let check_url = |r| check_url(&client, r);
            // TODO: eventually, this should be a checker that uses a cache, rather than
            // a cache that uses a checker.
            let response = cache
                .handle(&client, &cache_exclude_status, &accept, request, check_url)
                .await;
            (queue, response)
        })
        .buffer_unordered(max_concurrency);

    let responses = futures::stream::select(check_responses, request_building_errors);

    // Increment stats and extract recursive uris from responses. Each discovered
    // child must carry a clone of `queue` to stay outstanding; dropping `queue`
    // here (the common, non-recursive case) releases this request's liveness.
    let recursive_uris = responses.map(|(queue, response)| -> Vec<(RequestQueue, Request)> {
        progress.update(Some(response.body()));
        stats.add(response);

        let recursive_uris = vec![]; // currently unused.

        let _ = &queue;
        recursive_uris
    });

    // Send recursive uris back to the initial channel. This will terminate
    // only when all requests are finished and all `RequestQueue` handles are dropped.
    let all_done = recursive_uris.flat_map(stream::iter).for_each(
        async |(queue, req): (RequestQueue, Request)| {
            progress.inc_length(1);
            queue.enqueue(req).await.unwrap_or_else(|e| {
                warn!("unable to send recursive uri {:?} - channel closed?", e.0.1);
            });
        },
    );

    let start = std::time::Instant::now();

    /* Setup complete and streams ready. Starting execution */

    // This `await` is where execution begins. All streams are polled concurrently
    // and we wait for `all_done` or an early return with an error value.
    // WARNING: Before changing the `.await` structure, be aware of the
    // requirements imposed by [`lychee_lib::async_lib::stream::partition_result`].
    // Partitioned streams must be concurrently polled. At the moment, this is
    // achieved because all partitioned streams are within `all_done`.
    match futures::future::select(pin!(all_done), fatal_errors.next()).await {
        Either::Left(((), _fatal_errors)) => (),
        Either::Right((None, remaining)) => remaining.await,
        Either::Right((Some((_queue, fatal_error)), _remaining)) => {
            progress.finish("Error while fetching initial inputs");
            return Err(fatal_error);
        }
    }

    /* Main execution finished. Finalise stats and archive suggestions */

    progress.finish("Finished processing links");
    stats.duration = start.elapsed();

    if cfg.suggest() {
        let progress = Progress::new("Searching for alternatives", hide_bar, level, &cfg.mode());
        let archive = cfg.archive();
        let timeout = cfg.timeout();
        suggest_archived_links(archive, &mut stats, progress, max_concurrency, timeout).await;
    }

    let is_success = match cfg.accept_timeouts() {
        true => stats.is_success_ignoring_timeouts(),
        false => stats.is_success(),
    };
    let code = match is_success {
        true => ExitCode::Success,
        false => ExitCode::LinkCheckFailure,
    };
    Ok((stats, cache, code, client.host_pool()))
}

/// Ride-along handle for the recursive request channel.
///
/// Every in-flight request carries one. Holding it keeps the recursive channel
/// open, so the run continues until the last handle is dropped, at which point
/// the channel closes and the pipeline terminates.
///
/// Recursively discovered links are fed back in via [`RequestQueue::enqueue`],
/// which moves the handle onto the child so it stays open.
#[derive(Clone)]
struct RequestQueue(mpsc::Sender<(RequestQueue, Request)>);

impl RequestQueue {
    /// Enqueues a recursively discovered request
    async fn enqueue(
        &self,
        request: Request,
    ) -> Result<(), mpsc::error::SendError<(RequestQueue, Request)>> {
        let queue = self.clone();
        self.0.send((queue, request)).await
    }
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
            match future.await {
                Ok(Some(suggestion)) => {
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
                // No snapshot exists for this URL; nothing to suggest.
                Ok(None) => {}
                // The archive lookup itself failed (rate limiting, 5xx,
                // timeout, ...). Surface it so users understand why a
                // suggestion is missing (rather than silently dropping it).
                Err(e) => {
                    log::warn!("Failed to get archive snapshot for {}: {e}", url.as_str());
                }
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
            Status::Error(ErrorKind::InvalidURI(uri)),
            None,
            None,
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
        let (status, _redirects) = client.check_website(&uri).await;
        assert!(matches!(
            status,
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
