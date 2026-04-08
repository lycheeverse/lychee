use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::FutureExt;
use futures::SinkExt;
use futures::Stream;
use futures::StreamExt;
use futures::TryFutureExt;
use futures::TryStreamExt;
use futures::never::Never;
use futures::stream;
use http::StatusCode;
use log::warn;
use lychee_lib::ratelimit::HostPool;
use reqwest::Url;
use tokio::pin;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::Status;
use lychee_lib::archive::Archive;
use lychee_lib::waiter::{WaitGroup, WaitGuard};
use lychee_lib::{Client, ErrorKind, Request, Response};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::progress::Progress;
use crate::{ExitCode, cache::Cache};

use super::CommandParams;

pub(crate) async fn check2(
    params: CommandParams<impl Stream<Item = Result<Request, RequestError>>>,
) -> Result<(ResponseStats, Cache, ExitCode, Arc<HostPool>), ErrorKind> {
    let CommandParams {
        client,
        cache,
        requests,
        cfg,
        is_stdin_input,
    } = params;

    let max_concurrency = cfg.max_concurrency();

    let level = cfg.verbose().log_level();
    let hide_bar = cfg.no_progress || is_stdin_input;

    let accept = cfg.accept().into();
    let cache_exclude_status = cfg.cache_exclude_status().into();

    let (send_recursive_req_unused, recv_recursive_req) =
        mpsc::unbounded_channel::<(WaitGuard, Request)>();
    let send_recursive_req = &send_recursive_req_unused;

    let progress = Progress::new("Extracting links", hide_bar, level, &cfg.mode());
    let progress = &progress;

    let mut stats_owned = if cfg.verbose().log_level() >= log::Level::Info {
        ResponseStats::extended()
    } else {
        ResponseStats::default()
    };
    let stats = &mut stats_owned;

    let (waiter, wait_guard) = WaitGroup::new();

    // note that this stream closure OWNs a wait guard, so we rely on the stream being dropped
    // after it's finished to drop its wait guard.
    let initial_requests = requests.map(
        move |request| -> (WaitGuard, Result<Request, RequestError>) {
            progress.inc_length(1);
            (wait_guard.clone(), request)
        },
    );

    let recursive_sink = futures::sink::drain().with(
        |(guard, req)| -> futures::future::Ready<Result<(), Never>> {
            progress.inc_length(1);
            match send_recursive_req.send((guard, req)) {
                Ok(()) => (),
                Err(e) => {
                    warn!("unable to send recursive {:?} - channel closed?", e.0);
                }
            };

            futures::future::ok(())
        },
    );
    let recursive_sink = &recursive_sink;

    let recursive_receiver_stream = UnboundedReceiverStream::new(recv_recursive_req)
        .map(|(guard, req)| (guard, Ok(req)))
        .take_until(waiter.wait());

    // prefer to take from recursive requests before input requests. input
    // requests could come from a dir or glob walker which is computed on-demand,
    // whereas recursive requests exist in the queue and take memory.
    let prefer_left = |_: &mut ()| stream::PollNext::Left;
    let combined_requests =
        stream::select_with_strategy(recursive_receiver_stream, initial_requests, prefer_left);

    // perform requests. note this is the only part of this function that happens concurrently.
    let responses = combined_requests
        .map(
            async |(guard, request)| -> Result<(WaitGuard, Response), ErrorKind> {
                let response =
                    handle(&client, &cache, &cache_exclude_status, request, &accept).await;
                response.map(|response| (guard, response))
            },
        )
        .buffer_unordered(max_concurrency);

    let recursive_uris = responses.map_ok(|(guard, response)| -> Vec<(WaitGuard, Request)> {
        progress.update(Some(response.body()));
        stats.add(response);

        let recursive_uris = vec![];
        let _ = guard;

        recursive_uris
    });

    // send recursive uris back to the sink. try_for_each returns immediately if a
    // top-level error is encountered.
    recursive_uris
        .try_for_each(async move |uris| {
            let iter = uris.into_iter().map(Ok);
            let recursive_sink = recursive_sink.clone();
            pin!(recursive_sink);
            let Ok(()) = recursive_sink.send_all(&mut stream::iter(iter)).await;
            Ok(())
        })
        .await?;

    Err(ErrorKind::InvalidUrlHost)
}

pub(crate) async fn check<S>(
    params: CommandParams<S>,
) -> Result<(ResponseStats, Cache, ExitCode, Arc<HostPool>), ErrorKind>
where
    S: futures::Stream<Item = Result<Request, RequestError>>,
{
    // Setup
    let max_concurrency = params.cfg.max_concurrency();
    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, recv_resp) = mpsc::channel(max_concurrency);
    let (waiter, wait_guard) = WaitGroup::new();

    let start = std::time::Instant::now(); // Measure check time

    let stats = if params.cfg.verbose().log_level() >= log::Level::Info {
        ResponseStats::extended()
    } else {
        ResponseStats::default()
    };

    let client = params.client;
    let cache = params.cache;
    let cache_exclude_status = params.cfg.cache_exclude_status().into();
    let accept = params.cfg.accept().into();
    let accept_timeouts = params.cfg.accept_timeouts;

    // Start receiving requests
    let request_handle = tokio::spawn(request_channel_task(
        recv_req,
        send_resp,
        max_concurrency,
        client,
        cache,
        cache_exclude_status,
        accept,
    ));

    let hide_bar = params.cfg.no_progress || params.is_stdin_input;
    let level = params.cfg.verbose().log_level();

    let progress = Progress::new("Extracting links", hide_bar, level, &params.cfg.mode());
    let stats_handle = tokio::spawn(collect_responses(
        recv_resp,
        send_req.clone(),
        waiter,
        progress.clone(),
        stats,
    ));

    // Send requests into the channel. Note that this will run within the main task
    // and doesn't start until awaited.
    let send_task = send_requests(params.requests, wait_guard, send_req, &progress);

    // Waits for all futures to finish, either normally or due to panic.
    let (stats_result, request_result, send_result) =
        futures::join!(stats_handle, request_handle, send_task);

    // Fatal user errors are here so check it first.
    let mut stats: ResponseStats = stats_result??;

    let (cache, client) = request_result?;
    send_result.expect("sending input requests failed");

    stats.duration = start.elapsed();

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    progress.finish("Finished extracting links");

    if params.cfg.suggest {
        let progress = Progress::new(
            "Searching for alternatives",
            hide_bar,
            level,
            &params.cfg.mode(),
        );
        suggest_archived_links(
            params.cfg.archive(),
            &mut stats,
            progress,
            max_concurrency,
            params.cfg.timeout(),
        )
        .await;
    }

    let is_success = if accept_timeouts {
        stats.is_success_ignoring_timeouts()
    } else {
        stats.is_success()
    };
    let code = if is_success {
        ExitCode::Success
    } else {
        ExitCode::LinkCheckFailure
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

// drops the `send_req` channel on exit
// required for the receiver task to end, which closes send_resp, which allows
// the show_results_task to finish
async fn send_requests(
    requests: impl futures::Stream<Item = Result<Request, RequestError>>,
    guard: WaitGuard,
    send_req: mpsc::Sender<(WaitGuard, Result<Request, RequestError>)>,
    progress: &Progress,
) -> Result<(), ()> {
    tokio::pin!(requests);
    while let Some(request) = requests.next().await {
        progress.inc_length(1);
        send_req
            .send((guard.clone(), request))
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

/// Reads from the request channel and updates the progress bar status
async fn collect_responses(
    recv_resp: mpsc::Receiver<(WaitGuard, Result<Response, ErrorKind>)>,
    send_req: mpsc::Sender<(WaitGuard, Result<Request, RequestError>)>,
    waiter: WaitGroup,
    progress: Progress,
    mut stats: ResponseStats,
) -> Result<ResponseStats, ErrorKind> {
    // Wrap recv_resp until the WaitGroup finishes, at which time the
    // recv_resp_until_done stream will be closed. The correctness of
    // WaitGroup guarantees that if the waiter finishes, every channel
    // with a WaitGuard must be empty.
    let mut recv_resp_until_done = ReceiverStream::new(recv_resp)
        .take_until(waiter.wait())
        .boxed();

    while let Some((_guard, response)) = recv_resp_until_done.next().await {
        let response = response?;
        progress.update(Some(response.body()));
        stats.add(response);
    }

    // unused for now, but will be used for recursion eventually. by holding
    // an extra `send_req` endpoint, we prevent the natural termination when
    // each channel finishes and closes. instead, we rely on the WaitGroup to
    // break the cyclic channels.
    let _ = send_req;
    Ok(stats)
}

async fn request_channel_task(
    recv_req: mpsc::Receiver<(WaitGuard, Result<Request, RequestError>)>,
    send_resp: mpsc::Sender<(WaitGuard, Result<Response, ErrorKind>)>,
    max_concurrency: usize,
    client: Client,
    cache: Cache,
    cache_exclude_status: HashSet<StatusCode>,
    accept: HashSet<StatusCode>,
) -> (Cache, Client) {
    StreamExt::for_each_concurrent(
        ReceiverStream::new(recv_req),
        max_concurrency,
        |(guard, request): (WaitGuard, Result<Request, RequestError>)| async {
            let response = handle(&client, &cache, &cache_exclude_status, request, &accept).await;

            send_resp
                .send((guard, response))
                .await
                .expect("cannot send response to queue");
        },
    )
    .await;

    (cache, client)
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

/// Handle a single request
///
/// # Errors
///
/// An Err is returned if and only if there was an error while loading
/// a *user-provided* input argument. Other errors, including errors in
/// link resolution and in resolved inputs, will be returned as Ok with
/// a failed response.
async fn handle(
    client: &Client,
    cache: &Cache,
    cache_exclude_status: &HashSet<StatusCode>,
    request: Result<Request, RequestError>,
    accept: &HashSet<StatusCode>,
) -> Result<Response, ErrorKind> {
    // Note that the RequestError cases bypass the cache.
    let request = match request {
        Ok(x) => x,
        Err(e) => return e.into_response(),
    };

    cache
        .handle(client, cache_exclude_status, accept, request, |request| {
            check_url(client, request)
        })
        .await
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
        let response = handle(
            &client,
            &cache,
            &StatusCodeSelector::empty().into(),
            Ok(Request::try_from("https://wikipedia.org/").unwrap()),
            &StatusCodeSelector::default_accepted().into(),
        )
        .await
        .unwrap();
        assert!(response.status().is_error());
        assert!(cache.contains_key(&Uri::try_from("https://wikipedia.org/404").unwrap()));
    }
}
