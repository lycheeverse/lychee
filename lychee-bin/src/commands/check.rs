use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::StreamExt;
use http::StatusCode;
use lychee_lib::StatusCodeSelector;
use lychee_lib::ratelimit::HostPool;
use reqwest::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::archive::Archive;
use lychee_lib::{Client, ErrorKind, Request, Response, Uri};
use lychee_lib::{ResponseBody, Status};

use crate::formatters::get_progress_formatter;
use crate::formatters::response::ResponseFormatter;
use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::parse::parse_duration_secs;
use crate::progress::Progress;
use crate::{ExitCode, cache::Cache};

use super::CommandParams;

pub(crate) async fn check<S>(
    params: CommandParams<S>,
) -> Result<(ResponseStats, Cache, ExitCode, Arc<HostPool>), ErrorKind>
where
    S: futures::Stream<Item = Result<Request, RequestError>>,
{
    // Setup
    let (send_req, recv_req) = mpsc::channel(params.cfg.max_concurrency);
    let (send_resp, recv_resp) = mpsc::channel(params.cfg.max_concurrency);
    let max_concurrency = params.cfg.max_concurrency;

    // Measure check time
    let start = std::time::Instant::now();

    let stats = if params.cfg.verbose.log_level() >= log::Level::Info {
        ResponseStats::extended()
    } else {
        ResponseStats::default()
    };

    let client = params.client;
    let cache = params.cache;
    let cache_exclude_status = params
        .cfg
        .cache_exclude_status
        .unwrap_or(StatusCodeSelector::empty())
        .into();
    let accept = params
        .cfg
        .accept
        .unwrap_or(StatusCodeSelector::default_accepted())
        .into();

    // Start receiving requests
    let handle = tokio::spawn(request_channel_task(
        recv_req,
        send_resp,
        max_concurrency,
        client,
        cache,
        cache_exclude_status,
        accept,
    ));

    let hide_bar = params.cfg.no_progress;
    let detailed = params.cfg.verbose.log_level() >= log::Level::Info;

    let progress = Progress::new("Extracting links", hide_bar, detailed);
    let show_results_task = tokio::spawn(collect_responses(
        recv_resp,
        progress.clone(),
        get_progress_formatter(&params.cfg.mode),
        stats,
    ));

    // Wait until all requests are sent
    send_requests(params.requests, send_req, &progress).await?;
    let (cache, client) = handle.await?;

    // Wait until all responses are received
    let result = show_results_task.await?;
    let mut stats = result?;
    stats.duration_secs = start.elapsed().as_secs();

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    progress.finish("Finished extracting links");

    if params.cfg.suggest {
        let progress = Progress::new("Searching for alternatives", hide_bar, detailed);
        suggest_archived_links(
            params.cfg.archive.unwrap_or_default(),
            &mut stats,
            progress,
            max_concurrency,
            parse_duration_secs(params.cfg.timeout),
        )
        .await;
    }

    let code = if stats.is_success() {
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
async fn send_requests<S>(
    requests: S,
    send_req: mpsc::Sender<Result<Request, RequestError>>,
    progress: &Progress,
) -> Result<(), ErrorKind>
where
    S: futures::Stream<Item = Result<Request, RequestError>>,
{
    tokio::pin!(requests);
    while let Some(request) = requests.next().await {
        progress.inc_length(1);
        send_req.send(request).await.expect("Cannot send request");
    }
    Ok(())
}

/// Reads from the request channel and updates the progress bar status
async fn collect_responses(
    mut recv_resp: mpsc::Receiver<Result<Response, ErrorKind>>,
    progress: Progress,
    formatter: Box<dyn ResponseFormatter>,
    mut stats: ResponseStats,
) -> Result<ResponseStats, ErrorKind> {
    while let Some(response) = recv_resp.recv().await {
        let response = response?;
        let out = formatter.format_response(response.body());
        progress.update(Some(out));
        stats.add(response);
    }
    Ok(stats)
}

async fn request_channel_task(
    recv_req: mpsc::Receiver<Result<Request, RequestError>>,
    send_resp: mpsc::Sender<Result<Response, ErrorKind>>,
    max_concurrency: usize,
    client: Client,
    cache: Cache,
    cache_exclude_status: HashSet<StatusCode>,
    accept: HashSet<StatusCode>,
) -> (Cache, Client) {
    StreamExt::for_each_concurrent(
        ReceiverStream::new(recv_req),
        max_concurrency,
        |request: Result<Request, RequestError>| async {
            let response = handle(
                &client,
                &cache,
                cache_exclude_status.clone(),
                request,
                accept.clone(),
            )
            .await;

            send_resp
                .send(response)
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
    client.check(request).await.unwrap_or_else(|e| {
        log::error!("Error checking URL {uri}: Cannot parse URL to URI: {e}");
        Response::new(
            uri.clone(),
            Status::Error(ErrorKind::InvalidURI(uri.clone())),
            source.into(),
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

        return Ok(Response::new(uri.clone(), status, request.source.into()));
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
        .flat_map(|(source, set)| {
            set.iter()
                .map(move |ResponseBody { uri, status: _ }| (source, uri))
        })
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
