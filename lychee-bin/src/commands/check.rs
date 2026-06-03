use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

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
use lychee_lib::{Client, ErrorKind, Request, Response};

use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::progress::Progress;
use crate::{ExitCode, cache::Cache};

use super::CommandParams;

struct PendingRequest {
    // TODO this could option to be even light if recursion is not used
    /// Ride along sender used to enqueue recursively discovered children.
    ///
    /// By passing this along with the request/response we can rely on native
    /// rust ownership to prevent deadlocks and automatic cleanup
    send_req: mpsc::Sender<PendingRequest>,
    request: Result<Request, RequestError>,
}

struct PendingResponse {
    send_req: mpsc::Sender<PendingRequest>,
    response: Result<Response, ErrorKind>,
}

pub(crate) async fn check<S>(
    params: CommandParams<S>,
) -> Result<(ResponseStats, Cache, ExitCode, Arc<HostPool>), ErrorKind>
where
    S: futures::Stream<Item = Result<Request, RequestError>>,
{
    // Setup
    let max_concurrency = params.cfg.max_concurrency().get();
    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, recv_resp) = mpsc::channel(max_concurrency);

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
    let accept_timeouts = params.cfg.accept_timeouts();

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

    let hide_bar = params.cfg.no_progress() || params.is_stdin_input;
    let level = params.cfg.verbose().log_level();

    let progress = Progress::new("Extracting links", hide_bar, level, &params.cfg.mode());
    let stats_handle = tokio::spawn(collect_responses(recv_resp, progress.clone(), stats));

    // Send requests into the channel. Note that this will run within the main task
    // and doesn't start until awaited.
    let send_task = send_requests(params.requests, send_req, &progress);

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

    if params.cfg.suggest() {
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

// Drops the `send_req` channel on exit. This releases the initial
// outstanding work token: once every queued/in-flight item has also dropped
// its ride along sender and the request channel closes, which closes send_resp,
// which lets `collect_responses` finish.
async fn send_requests(
    requests: impl futures::Stream<Item = Result<Request, RequestError>>,
    send_req: mpsc::Sender<PendingRequest>,
    progress: &Progress,
) -> Result<(), ()> {
    tokio::pin!(requests);
    while let Some(request) = requests.next().await {
        progress.inc_length(1);
        send_req
            .send(PendingRequest {
                send_req: send_req.clone(),
                request,
            })
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

/// Reads from the response channel and updates the progress bar status.
///
/// Each item carries the ride-along request sender (see [`PendingRequest`]).
/// The sender is dropped at the end of each iteration once the response has
/// been handled. When the last one drops, the request channel closes and the
/// pipeline terminates. To recurse, clone `send_req` onto each discovered
/// child before it is dropped here.
async fn collect_responses(
    recv_resp: mpsc::Receiver<PendingResponse>,
    progress: Progress,
    mut stats: ResponseStats,
) -> Result<ResponseStats, ErrorKind> {
    let mut recv_resp = ReceiverStream::new(recv_resp);

    while let Some(PendingResponse { send_req, response }) = recv_resp.next().await {
        let response = response?;
        progress.update(Some(response.body()));
        stats.add(response);

        // TODO: recurse by enqueuing discovered child URLs via `send_req`,
        // cloning it onto each child before this `send_req` is dropped below.
        let _ = &send_req;
    }

    Ok(stats)
}

async fn request_channel_task(
    recv_req: mpsc::Receiver<PendingRequest>,
    send_resp: mpsc::Sender<PendingResponse>,
    max_concurrency: usize,
    client: Client,
    cache: Cache,
    cache_exclude_status: HashSet<StatusCode>,
    accept: HashSet<StatusCode>,
) -> (Cache, Client) {
    StreamExt::for_each_concurrent(
        ReceiverStream::new(recv_req),
        max_concurrency,
        |PendingRequest { send_req, request }: PendingRequest| async {
            let response = handle(
                &client,
                &cache,
                cache_exclude_status.clone(),
                request,
                &accept,
            )
            .await;

            // The ride along sender moves onto the response, so it stays alive
            // across the whole check window
            send_resp
                .send(PendingResponse { send_req, response })
                .await
                .unwrap_or_default();
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
            Status::Error(ErrorKind::InvalidURI(uri)),
            None,
            None,
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
    accept: &HashSet<StatusCode>,
) -> Result<Response, ErrorKind> {
    // Note that the RequestError cases bypass the cache.
    let request = match request {
        Ok(x) => x,
        Err(e) => return e.into_response(),
    };

    let check = async |request: Request| check_url(client, request).await;

    cache
        .handle(client, cache_exclude_status, accept, request, check)
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
        let response = handle(
            &client,
            &cache,
            StatusCodeSelector::empty().into(),
            Ok(Request::try_from("https://wikipedia.org/").unwrap()),
            &StatusCodeSelector::default_accepted().into(),
        )
        .await
        .unwrap();
        assert!(response.status().is_error());
        assert!(cache.contains_key(&Uri::try_from("https://wikipedia.org/404").unwrap()));
    }
}
