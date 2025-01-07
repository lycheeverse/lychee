use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{task, StreamExt};
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use reqwest::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use lychee_lib::{BasicAuthCredentials, Client, ErrorKind, Request, Response, Uri};
use lychee_lib::{InputSource, Result};
use lychee_lib::{ResponseBody, Status};

use crate::archive::{Archive, Suggestion};
use crate::formatters::get_response_formatter;
use crate::formatters::response::ResponseFormatter;
use crate::parse::parse_duration_secs;
use crate::verbosity::Verbosity;
use crate::{cache::Cache, stats::ResponseStats, ExitCode};

use super::CommandParams;

pub(crate) async fn check<S>(
    params: CommandParams<S>,
) -> Result<(ResponseStats, Arc<Cache>, ExitCode)>
where
    S: futures::Stream<Item = Result<Request>>,
{
    // Setup
    let max_concurrency = params.cfg.max_concurrency;
    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, recv_resp) = mpsc::channel(max_concurrency);
    let remaining_requests = Arc::new(AtomicUsize::new(0));

    // Measure check time
    let start = std::time::Instant::now();

    let stats = if params.cfg.verbose.log_level() >= log::Level::Info {
        ResponseStats::extended()
    } else {
        ResponseStats::default()
    };
    let cache_ref = params.cache.clone();

    let client = params.client;
    let cache = params.cache;
    let cache_exclude_status = params.cfg.cache_exclude_status.into_set();
    let accept = params.cfg.accept.into_set();

    let pb = if params.cfg.no_progress || params.cfg.verbose.log_level() >= log::Level::Info {
        None
    } else {
        Some(init_progress_bar("Extracting links"))
    };

    // Start receiving requests
    tokio::spawn(request_channel_task(
        recv_req,
        send_resp,
        max_concurrency,
        client,
        cache,
        cache_exclude_status,
        accept,
    ));

    let formatter = get_response_formatter(&params.cfg.mode);

    let show_results_task = tokio::spawn(response_receive_task(
        recv_resp,
        send_req.clone(),
        remaining_requests.clone(),
        params.cfg.max_depth,
        params.cfg.recursive,
        params.cfg.verbose,
        pb.clone(),
        formatter,
        stats,
    ));

    // Fill the request channel with the initial requests
    send_inputs_loop(
        params.requests,
        send_req,
        pb,
        remaining_requests,
        params.cfg.max_depth,
    )
    .await?;

    // Wait until all responses are received
    let result = show_results_task.await?;
    let (pb, mut stats) = result?;

    // Store elapsed time in stats
    stats.duration_secs = start.elapsed().as_secs();

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb {
        pb.finish_with_message("Finished extracting links");
    }

    if params.cfg.suggest {
        suggest_archived_links(
            params.cfg.archive.unwrap_or_default(),
            &mut stats,
            !params.cfg.no_progress,
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
    Ok((stats, cache_ref, code))
}

async fn suggest_archived_links(
    archive: Archive,
    stats: &mut ResponseStats,
    show_progress: bool,
    max_concurrency: usize,
    timeout: Duration,
) {
    let failed_urls = &get_failed_urls(stats);
    let bar = if show_progress {
        let bar = init_progress_bar("Searching for alternatives");
        bar.set_length(failed_urls.len() as u64);
        Some(bar)
    } else {
        None
    };

    let suggestions = Mutex::new(&mut stats.suggestion_map);

    futures::stream::iter(failed_urls)
        .map(|(input, url)| (input, url, archive.get_link(url, timeout)))
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

            if let Some(bar) = &bar {
                bar.inc(1);
            }
        })
        .await;

    if let Some(bar) = &bar {
        bar.finish_with_message("Finished searching for alternatives");
    }
}

// drops the `send_req` channel on exit
// required for the receiver task to end, which closes send_resp, which allows
// the show_results_task to finish
async fn send_inputs_loop<S>(
    requests: S,
    send_req: mpsc::Sender<Result<Request>>,
    bar: Option<ProgressBar>,
    remaining_requests: Arc<AtomicUsize>,
    max_recursion_depth: Option<usize>,
) -> Result<()>
where
    S: futures::Stream<Item = Result<Request>>,
{
    tokio::pin!(requests);
    println!("--- INITIAL REQUESTS ---");
    let mut i = 0;
    while let Some(request) = requests.next().await {
        // println!("#{} starting request", i);
        i += 1;
        let request = request?;

        if max_recursion_depth
            .map(|limit| request.recursion_level > limit)
            .unwrap_or(false)
        {
            continue;
        }

        if let Some(pb) = &bar {
            pb.inc_length(1);
            pb.set_message(request.to_string());
        };
        remaining_requests.fetch_add(1, Ordering::Relaxed);
        let uri = request.uri.clone();
        // println!("sending request to queue for {}", uri);
        send_req
            .send(Ok(request))
            .await
            .expect("Cannot send request");
        // println!("sent request to queue for {}", uri);
    }
    Ok(())
}

/// Reads from the response channel, updates the progress bar status and (if recursing) sends new requests.
async fn response_receive_task(
    mut recv_resp: mpsc::Receiver<Response>,
    req_send: mpsc::Sender<Result<Request>>,
    remaining_requests: Arc<AtomicUsize>,
    max_recursion_depth: Option<usize>,
    recurse: bool,
    verbose: Verbosity,
    pb: Option<ProgressBar>,
    formatter: Box<dyn ResponseFormatter>,
    mut stats: ResponseStats,
) -> Result<(Option<ProgressBar>, ResponseStats)> {
    let mut i = 0;
    while let Some(response) = recv_resp.recv().await {
        // println!(
        //     "starting response #{} out of {}",
        //     i,
        //     remaining_requests.load(Ordering::Relaxed),
        // );
        // println!("#{} received response from queue for {}", i, response.1.uri);
        i += 1;
        show_progress(
            &mut io::stderr(),
            pb.as_ref(),
            &response,
            formatter.as_ref(),
            &verbose,
        )?;

        if recurse
            && max_recursion_depth
                .map(|limit| response.1.recursion_level <= limit)
                .unwrap_or(true)
        {
            tokio::spawn((|requests: Vec<Request>,
                           req_send: mpsc::Sender<Result<Request>>,
                           remaining_requests: Arc<AtomicUsize>,
                           pb: Option<ProgressBar>| async move {
                for request in requests {
                    let uri = request.uri.clone().to_string();
                    req_send
                        .send(Ok(request))
                        .await
                        .expect("Cannot send request");
                    remaining_requests.fetch_add(1, Ordering::Relaxed);

                    if let Some(bar) = &pb {
                        bar.inc_length(1);
                        bar.set_message(uri);
                    }
                }
            })(
                response.subsequent_requests(None),
                req_send.clone(),
                remaining_requests.clone(),
                pb.clone(),
            ));
        }

        remaining_requests.fetch_sub(1, Ordering::Relaxed);
        let remaining_now = remaining_requests.load(Ordering::Relaxed);
        // println!("remaining requests: {}", remaining_now);
        if remaining_now == 0 {
            break;
        }

        stats.add(response);
        // println!("finished response #{}", i);
    }
    // println!("Processed {} responses", i);
    Ok((pb, stats))
}

fn init_progress_bar(initial_message: &'static str) -> ProgressBar {
    let bar = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template("{spinner:.162} {pos}/{len:.238} {bar:.162/238} {wide_msg}")
            .expect("Valid progress bar")
            .progress_chars("━ ━"),
    );
    bar.set_length(0);
    bar.set_message(initial_message);
    // report status _at least_ every 500ms
    bar.enable_steady_tick(Duration::from_millis(500));
    bar
}

async fn request_channel_task(
    recv_req: mpsc::Receiver<Result<Request>>,
    send_resp: mpsc::Sender<Response>,
    max_concurrency: usize,
    client: Client,
    cache: Arc<Cache>,
    cache_exclude_status: HashSet<u16>,
    accept: HashSet<u16>,
) {
    // while let Some(request) = recv_req.recv().await {
    StreamExt::for_each_concurrent(
        ReceiverStream::new(recv_req),
        max_concurrency,
        |request: Result<Request>| async {
            let request = request.expect("cannot read request");
            // let uri = request.uri.clone();
            // println!("handling request {}", uri);
            // let uri = request.uri.clone();
            // println!("received request for {}", uri);
            let response = handle(
                &client,
                cache.clone(),
                cache_exclude_status.clone(),
                request,
                accept.clone(),
            )
            .await;

            // println!("sending response to queue for {}", uri);
            send_resp
                .send(response)
                .await
                .expect("Cannot send response");
            // if let Err(_) = timeout(Duration::from_millis(500), send_resp.send(response)).await {
            //     println!(
            //         "Timeout occurred while sending response to queue for {}",
            //         uri
            //     );
            // }
            // println!("sent response to queue for {}", uri);
        },
    )
    .await;
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
        log::error!("Error checking URL {}: Cannot parse URL to URI: {}", uri, e);
        Response::new(
            uri.clone(),
            Status::Error(ErrorKind::InvalidURI(uri.clone())),
            source,
            vec![],
            0,
        )
    })
}

/// Handle a single request
async fn handle(
    client: &Client,
    cache: Arc<Cache>,
    cache_exclude_status: HashSet<u16>,
    request: Request,
    accept: HashSet<u16>,
) -> Response {
    let uri = request.uri.clone();
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
            Status::from_cache_status(v.value().status, &accept)
        };
        // TODO: not too sure about it, we never recurse on cached requests
        // println!("Found cached response for {}", uri);
        return Response::new(uri.clone(), status, request.source, vec![], 0);
    }

    // Request was not cached; run a normal check
    let response = check_url(client, request).await;

    // - Never cache filesystem access as it is fast already so caching has no
    //   benefit.
    // - Skip caching unsupported URLs as they might be supported in a
    //   future run.
    // - Skip caching excluded links; they might not be excluded in the next run.
    // - Skip caching links for which the status code has been explicitly excluded from the cache.
    let status = response.status();
    if ignore_cache(&uri, status, &cache_exclude_status) {
        return response;
    }

    cache.insert(uri, status.into());
    response
}

/// Returns `true` if the response should be ignored in the cache.
///
/// The response should be ignored if:
/// - The URI is a file URI.
/// - The status is excluded.
/// - The status is unsupported.
/// - The status is unknown.
/// - The status code is excluded from the cache.
fn ignore_cache(uri: &Uri, status: &Status, cache_exclude_status: &HashSet<u16>) -> bool {
    let status_code_excluded = status
        .code()
        .map_or(false, |code| cache_exclude_status.contains(&code.as_u16()));

    uri.is_file()
        || status.is_excluded()
        || status.is_unsupported()
        || status.is_unknown()
        || status_code_excluded
}

fn show_progress(
    output: &mut dyn Write,
    progress_bar: Option<&ProgressBar>,
    response: &Response,
    formatter: &dyn ResponseFormatter,
    verbose: &Verbosity,
) -> Result<()> {
    // In case the log level is set to info, we want to show the detailed
    // response output. Otherwise, we only show the essential information
    // (typically the status code and the URL, but this is dependent on the
    // formatter).
    let out = if verbose.log_level() >= log::Level::Info {
        formatter.format_detailed_response(response.body())
    } else {
        formatter.format_response(response.body())
    };

    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(out.clone());
        if verbose.log_level() >= log::Level::Info {
            pb.println(out);
        }
    } else if verbose.log_level() >= log::Level::Info
        || (!response.status().is_success() && !response.status().is_excluded())
    {
        writeln!(output, "{out}")?;
    }
    Ok(())
}

fn get_failed_urls(stats: &mut ResponseStats) -> Vec<(InputSource, Url)> {
    stats
        .error_map
        .iter()
        .flat_map(|(source, set)| {
            set.iter()
                .map(move |ResponseBody { uri, .. }| (source, uri))
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
    use crate::{formatters::get_response_formatter, options};
    use http::StatusCode;
    use log::info;
    use lychee_lib::{CacheStatus, ClientBuilder, ErrorKind, InputSource, Uri};

    use super::*;

    #[test]
    fn test_skip_cached_responses_in_progress_output() {
        let mut buf = Vec::new();
        let response = Response::new(
            Uri::try_from("http://127.0.0.1").unwrap(),
            Status::Cached(CacheStatus::Ok(200)),
            InputSource::Stdin,
            vec![],
            0,
        );
        let formatter = get_response_formatter(&options::OutputMode::Plain);
        show_progress(
            &mut buf,
            None,
            &response,
            formatter.as_ref(),
            &Verbosity::default(),
        )
        .unwrap();

        info!("{:?}", String::from_utf8_lossy(&buf));
        assert!(buf.is_empty());
    }

    #[test]
    fn test_show_cached_responses_in_progress_debug_output() {
        let mut buf = Vec::new();
        let response = Response::new(
            Uri::try_from("http://127.0.0.1").unwrap(),
            Status::Cached(CacheStatus::Ok(200)),
            InputSource::Stdin,
            vec![],
            0,
        );
        let formatter = get_response_formatter(&options::OutputMode::Plain);
        show_progress(
            &mut buf,
            None,
            &response,
            formatter.as_ref(),
            &Verbosity::debug(),
        )
        .unwrap();

        assert!(!buf.is_empty());
        let buf = String::from_utf8_lossy(&buf);
        assert_eq!(buf, "[200] http://127.0.0.1/ | OK (cached)\n");
    }

    #[tokio::test]
    async fn test_invalid_url() {
        let client = ClientBuilder::builder().build().client().unwrap();
        let uri = Uri::try_from("http://\"").unwrap();
        let (status, _) = client.check_website(&uri, None).await.unwrap();
        assert!(matches!(
            status,
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
        let exclude = [StatusCode::OK.as_u16()].iter().copied().collect();

        assert!(ignore_cache(
            &Uri::try_from("https://[::1]").unwrap(),
            &Status::Ok(StatusCode::OK),
            &exclude
        ));
    }
}
