use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::FutureExt;
use futures::StreamExt;
use http::StatusCode;
use lychee_lib::ratelimit::HostPool;
use reqwest::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::Status;
use lychee_lib::archive::Archive;
use lychee_lib::cache::CacheFut;
use lychee_lib::waiter::{WaitGroup, WaitGuard};
use lychee_lib::{Client, ErrorKind, Request, Response};

use crate::ExitCode;
use crate::cache::{CacheValue, LycheeCache};
use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::progress::Progress;

use super::CommandParams;

pub(crate) async fn check<S>(
    params: CommandParams<S>,
) -> Result<(ResponseStats, LycheeCache, ExitCode, Arc<HostPool>), ErrorKind>
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
    let accept = params.cfg.accept().into();
    let accept_timeouts = params.cfg.accept_timeouts;

    // Start receiving requests
    let request_handle = tokio::spawn(request_channel_task(
        recv_req,
        send_resp,
        max_concurrency,
        client,
        cache,
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
    cache: LycheeCache,
    accept: HashSet<StatusCode>,
) -> (LycheeCache, Client) {
    let (send_side_channel, recv_side_channel) = mpsc::channel(max_concurrency * 100);

    let main_task = {
        // It's vital that we MOVE send_side_channel into the for_each_concurrent
        // closure, otherwise these two tasks will deadlock. To do this, we use
        // `move` on the closure and async block, and we shadow the other
        // variables so they will still be captured by reference.
        let send_side_channel = send_side_channel;
        let send_resp = &send_resp;
        let client = &client;
        let cache = &cache;

        StreamExt::for_each_concurrent(
            ReceiverStream::new(recv_req),
            max_concurrency,
            move |(guard, request)| {
                // We also have to clone within this closure, because each invocation of
                // the closure returns a new future.
                let send_side_channel = send_side_channel.clone();
                handle(
                    client,
                    cache,
                    guard,
                    request,
                    send_resp.clone(),
                    send_side_channel,
                )
            },
        )
        .boxed()
    };

    // Note that the cached handler task is not concurrent, because it only does
    // very simple computations and we don't want to take resources away from the
    // main task.
    // TODO: should this be concurrent? might become a bottleneck if there are lots
    // of cache hits.
    let side_task = StreamExt::for_each(
        ReceiverStream::new(recv_side_channel),
        |(guard, request, once)| async {
            let response = handle_cached(&client, &accept, request, once).await;

            send_resp
                .send((guard, Ok(response)))
                .await
                .expect("cannot send response to queue from side task");
        },
    )
    .boxed();

    let ((), ()) = futures::join!(main_task, side_task);

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

/// Handles a single request by checking the URL and ensures that there are no
/// simultaneous requests for the same [`Uri`]. This function does not return
/// a value. Instead, it forwards to either the `send_resp` or `send_side`
/// channels.
///
/// If the request URL has not been seen before, it will mark the request as
/// in-progress in the cache, then perform the link check. Afterwards, it will
/// update the cache with the response and forward the response to the `send_resp`
/// channel.
///
/// If the request URL is in-progress (or previously finished), this function
/// will skip link checking and forward the request to the `send_side` channel,
/// along with a future which can be used to wait for the result (or fetch
/// the cached result).
///
/// # Errors
///
/// An Err is forwarded to the `send_resp` channel if and only if there was an
/// error while loading a *user-provided* input argument. Other errors, including
/// errors in link resolution and in resolved inputs, will be forwarded as Ok
/// with a failed response.
async fn handle(
    client: &Client,
    cache: &LycheeCache,
    guard: WaitGuard,
    request: Result<Request, RequestError>,
    send_resp: mpsc::Sender<(WaitGuard, Result<Response, ErrorKind>)>,
    send_side: mpsc::Sender<(WaitGuard, Request, CacheFut<CacheValue>)>,
) {
    // Note that the RequestError cases bypass the cache.
    let request = match request {
        Ok(x) => x,
        Err(e) => {
            return send_resp
                .send((guard, e.into_response()))
                .await
                .expect("response channel closed");
        }
    };

    // First check the persistent in-memory cache
    match cache.lock_entry(&request.uri) {
        Ok(setter) => {
            println!("main");
            let response = check_url(client, request).await;
            setter.set(response.status().into());
            send_resp
                .send((guard, Ok(response)))
                .await
                .expect("response channel closed");
        }
        // Found a cached request
        Err(fut) => {
            send_side
                .send((guard, request, fut))
                .await
                .expect("side channel closed");
        }
    }
}

async fn handle_cached(
    client: &Client,
    accept: &HashSet<StatusCode>,
    request: Request,
    once: CacheFut<CacheValue>,
) -> Response {
    println!("side waiting");
    let uri = request.uri;
    let status = once.wait().await.status;
    println!("side ready");

    // Overwrite cache status in case the URI is excluded in the
    // current run
    let status = if client.is_excluded(&uri) {
        Status::Excluded
    } else {
        // Can't impl `Status::from(v.value().status)` here because the
        // `accepted` status codes might have changed from the previous run
        // and they may have an impact on the interpretation of the status
        // code.
        client.host_pool().record_persistent_cache_hit(&uri);
        Status::from_cache_status(status, accept)
    };

    Response::new(uri, status, request.source.into(), request.span, None)
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
    use http::StatusCode;
    use lychee_lib::{ClientBuilder, ErrorKind, Uri};

    use super::*;

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

    #[test]
    fn test_cache_by_default() {
        assert!(
            LycheeCache::to_disk_cache_value((&Status::Ok(StatusCode::OK)).into(), &HashSet::new())
                .is_some()
        );
    }

    #[test]
    // Cache is ignored for file URLs
    fn test_cache_ignore_file_urls() {
        assert!(LycheeCache::is_bypassed_from_cache(
            &Uri::try_from("file:///home").unwrap(),
        ));
    }

    #[test]
    // Cache is ignored for unsupported status
    fn test_cache_ignore_unsupported_status() {
        assert!(
            LycheeCache::to_disk_cache_value(
                (&Status::Unsupported(ErrorKind::EmptyUrl)).into(),
                &HashSet::new()
            )
            .is_none()
        );
    }

    #[test]
    // Cache is ignored for unknown status
    fn test_cache_ignore_unknown_status() {
        assert!(
            LycheeCache::to_disk_cache_value(
                (&Status::UnknownStatusCode(StatusCode::IM_A_TEAPOT)).into(),
                &HashSet::new()
            )
            .is_none()
        );
    }
}
