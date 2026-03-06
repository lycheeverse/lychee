use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Poll;
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use http::StatusCode;
use lychee_lib::ratelimit::HostPool;
use reqwest::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::Status;
use lychee_lib::archive::Archive;
use lychee_lib::waiter::{WaitGroup, WaitGuard};
use lychee_lib::{Client, ErrorKind, Request, Response, Uri};

use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::progress::Progress;
use crate::{ExitCode, cache::Cache};

use super::CommandParams;

use futures::future::FusedFuture;

pin_project_lite::pin_project! {
    struct StoringFuture<Fut, T> {
        #[pin]
        fut: Pin<Box<Fut>>,

        stored: Arc<std::cell::OnceCell<T>>,
    }
}

impl<Fut, T> StoringFuture<Fut, T> {
    fn new(fut: Fut) -> StoringFuture<Fut, T>
    where
        Fut: futures::future::Future<Output = T>,
    {
        StoringFuture {
            fut: Box::pin(fut),
            stored: Default::default(),
        }
    }

    fn get(&self) -> Option<&T> {
        self.stored.get()
    }

    fn into_stored(self) -> Option<T> {
        Arc::into_inner(self.stored)
            .expect("arc should be unique, due to exclusive mutable borrow")
            .take()
    }

    async fn wait(self) -> T
    where
        Fut: futures::future::Future<Output = T>,
    {
        if let Some(_) = self.get() {
            return self.into_stored().expect("checked by .get");
        }
        let arc = self.stored.clone();
        self.await;
        Arc::into_inner(arc)
            .expect("arc should be unique, due to ownership of self and dropping of self")
            .take()
            .expect("value not set after future finished. did it panic?")
    }

    // async fn wait2(&mut self) -> impl Future<Output = &T>
    // where
    //     Fut: futures::future::FusedFuture<Output = T>,
    // {
    //     match self.stored.get() {
    //         Some(_) => return futures::future::ready(x).left_future(),
    //         None => (),
    //     };
    //     panic!("a")
    //     // std::future::poll_fn(move |cx| match self.as_mut().poll(cx) {
    //     //     Poll::Ready(()) => Poll::Ready(self.get().unwrap()),
    //     //     Poll::Pending => Poll::Pending
    //     // }).right_future()
    // }

    async fn wait_and_set(&mut self) -> &T
    where
        Fut: futures::future::Future<Output = T>,
    {
        Pin::new(&mut *self).await;
        self.stored
            .get()
            .expect("value not set after future finished. did it panic?")
    }
}

impl<Fut, T> FusedFuture for StoringFuture<Fut, T>
where
    Fut: futures::future::FusedFuture<Output = T>,
{
    fn is_terminated(&self) -> bool {
        self.fut.is_terminated()
    }
}

/// It is generally not useful to use `.await` directly on this future,
/// because that will consume the future and its stored cell. Instead,
/// you should pin this future and use `.await` on `Pin<&mut StoringFuture<>>`.
impl<Fut, T> Future for StoringFuture<Fut, T>
where
    Fut: futures::future::Future<Output = T>,
{
    type Output = ();
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let pinned = self.project();
        pinned.fut.poll(cx).map(|x| {
            let _ = pinned.stored.set(x);
            ()
        })
    }
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
    let abort_request = request_handle.abort_handle();
    let mut request_handle = StoringFuture::new(request_handle.fuse());

    let hide_bar = params.cfg.no_progress;
    let level = params.cfg.verbose().log_level();

    let progress = Progress::new("Extracting links", hide_bar, level, &params.cfg.mode());
    let stats_handle = tokio::spawn(collect_responses(
        recv_resp,
        send_req.clone(),
        waiter,
        progress.clone(),
        stats,
    ));
    let abort_stats = stats_handle.abort_handle();
    let mut stats_handle = StoringFuture::new(stats_handle.fuse());

    // Send requests into the channel.
    let send_task = send_requests(params.requests, wait_guard, send_req, &progress).fuse();
    tokio::pin!(send_task);
    let mut send_task = StoringFuture::new(send_task);

    // Note the differences between spawned tasks (with `tokio::spawn`) and async
    // subtasks within the main task:
    // - Spawned tasks are isolated from the main process. They can panic and their
    //   anic will *not* propagate to the toplevel, unlike panics in the main task.
    // - Spawned tasks are *automatically* started when `spawn` is called, unlike
    //   async subtasks which only start when awaited.
    // - Additionally, spawned tasks are not automatically cancelled on drop, so
    //   we *must* abort them manually to ensure termination.

    loop {
        // Race the futures so that if `stats_handle` finishes, we always process
        // it first and abort the others. This must be in a loop, in case another
        // task (e.g., `send_task`) finishes first.
        futures::select! {
            () = stats_handle => {
                log::debug!("Response processing task finished");
                if let Some(Ok(Err(_))) = stats_handle.get() {
                    let Some(Ok(Err(e))) = stats_handle.into_stored() else {
                        panic!("pattern match will succeed because we just checked it");
                    };
                    // very important to abort the spawned tasks, to ensure termination.
                    log::debug!("Fatal error, aborting other tasks");
                    abort_request.abort();
                    abort_stats.abort();
                    return Err(e);
                }
            }
            () = request_handle => log::debug!("Request handling task finished"),
            () = send_task => log::debug!("Request enqueueing task finished"),
            complete => break,
        };
    }
    log::debug!("All check tasks finished");

    // Waits for all futures to finish, either normally or due to panic.
    let (Some(stats_result), Some(request_result), Some(send_result)) = (
        stats_handle.into_stored(),
        request_handle.into_stored(),
        send_task.into_stored(),
    ) else {
        panic!("StoredFuture should all be ready due to earlier select.")
    };

    // Unwraps two results, the first with JoinError and the second with ErrorKind.
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
            let response = handle(
                &client,
                &cache,
                cache_exclude_status.clone(),
                request,
                accept.clone(),
            )
            .await;

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
        log::error!("Error checking URL {uri}: Cannot parse URL to URI: {e}");
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
    cache_exclude_status: HashSet<StatusCode>,
    request: Result<Request, RequestError>,
    accept: HashSet<StatusCode>,
) -> Result<Response, ErrorKind> {
    // Note that the RequestError cases bypass the cache.
    let request = match request {
        Ok(x) => x,
        Err(e) => return e.into_response(),
    };

    let uri = request.uri.clone();

    // First check the persistent disk-based cache
    if let Some(v) = cache.get(&uri) {
        // Found a cached request
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
            Status::from_cache_status(v.value().status, &accept)
        };

        return Ok(Response::new(
            uri.clone(),
            status,
            request.source.into(),
            request.span,
            None,
        ));
    }

    let response = check_url(client, request).await;

    // - Never cache filesystem access as it is fast already so caching has no benefit.
    // - Skip caching unsupported URLs as they might be supported in a future run.
    // - Skip caching excluded links; they might not be excluded in the next run.
    // - Skip caching links for which the status code has been explicitly excluded from the cache.
    let status = response.status();
    if ignore_cache(&uri, status, &cache_exclude_status) {
        return Ok(response);
    }

    cache.insert(uri, status.into());
    Ok(response)
}

/// Returns `true` if the response should be ignored in the cache.
///
/// The response should be ignored if:
/// - The URI is a file URI.
/// - The status is excluded.
/// - The status is unsupported.
/// - The status is unknown.
/// - The status code is excluded from the cache.
fn ignore_cache(uri: &Uri, status: &Status, cache_exclude_status: &HashSet<StatusCode>) -> bool {
    let status_code_excluded = status
        .code()
        .is_some_and(|code| cache_exclude_status.contains(&code));

    uri.is_file()
        || status.is_excluded()
        || status.is_unsupported()
        || status.is_unknown()
        || status_code_excluded
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
        assert!(!ignore_cache(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Ok(StatusCode::OK),
            &HashSet::default()
        ));
    }

    #[test]
    // Cache is ignored for file URLs
    fn test_cache_ignore_file_urls() {
        assert!(ignore_cache(
            &Uri::try_from("file:///home").unwrap(),
            &Status::Ok(StatusCode::OK),
            &HashSet::default()
        ));
    }

    #[test]
    // Cache is ignored for unsupported status
    fn test_cache_ignore_unsupported_status() {
        assert!(ignore_cache(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Unsupported(ErrorKind::EmptyUrl),
            &HashSet::default()
        ));
    }

    #[test]
    // Cache is ignored for unknown status
    fn test_cache_ignore_unknown_status() {
        assert!(ignore_cache(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::UnknownStatusCode(StatusCode::IM_A_TEAPOT),
            &HashSet::default()
        ));
    }

    #[test]
    fn test_cache_ignore_excluded_status() {
        // Cache is ignored for excluded status codes
        let exclude = HashSet::from([StatusCode::OK]);

        assert!(ignore_cache(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Ok(StatusCode::OK),
            &exclude
        ));
    }
}
