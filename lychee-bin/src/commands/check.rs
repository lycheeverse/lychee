use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use reqwest::Url;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use lychee_lib::{Client, ErrorKind, Request, Response};
use lychee_lib::{InputSource, Result};
use lychee_lib::{ResponseBody, Status};

use crate::archive::{Archive, Suggestion};
use crate::formatters::response::ResponseFormatter;
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
    let cache_ref = params.cache.clone();

    let client = params.client;
    let cache = params.cache;
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
        accept,
    ));

    let show_results_task = tokio::spawn(progress_bar_task(
        recv_resp,
        params.cfg.verbose,
        pb.clone(),
        Arc::new(params.formatter),
        stats,
    ));

    // Wait until all messages are sent
    send_inputs_loop(params.requests, send_req, pb).await?;

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
        .map(|(input, url)| (input, url, archive.get_link(url)))
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
) -> Result<()>
where
    S: futures::Stream<Item = Result<Request>>,
{
    tokio::pin!(requests);
    while let Some(request) = requests.next().await {
        let request = request?;
        if let Some(pb) = &bar {
            pb.inc_length(1);
            pb.set_message(request.to_string());
        };
        send_req
            .send(Ok(request))
            .await
            .expect("Cannot send request");
    }
    Ok(())
}

/// Reads from the request channel and updates the progress bar status
async fn progress_bar_task(
    mut recv_resp: mpsc::Receiver<Response>,
    verbose: Verbosity,
    pb: Option<ProgressBar>,
    formatter: Arc<Box<dyn ResponseFormatter>>,
    mut stats: ResponseStats,
) -> Result<(Option<ProgressBar>, ResponseStats)> {
    while let Some(response) = recv_resp.recv().await {
        show_progress(&mut io::stderr(), &pb, &response, &formatter, &verbose)?;
        stats.add(response);
    }
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
    accept: HashSet<u16>,
) {
    StreamExt::for_each_concurrent(
        ReceiverStream::new(recv_req),
        max_concurrency,
        |request: Result<Request>| async {
            let request = request.expect("cannot read request");
            let response = handle(&client, cache.clone(), request, accept.clone()).await;

            send_resp
                .send(response)
                .await
                .expect("cannot send response to queue");
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
        )
    })
}

/// Handle a single request
async fn handle(
    client: &Client,
    cache: Arc<Cache>,
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
        return Response::new(uri.clone(), status, request.source);
    }

    // Request was not cached; run a normal check
    let response = check_url(client, request).await;

    // - Never cache filesystem access as it is fast already so caching has no
    //   benefit.
    // - Skip caching unsupported URLs as they might be supported in a
    //   future run.
    // - Skip caching excluded links; they might not be excluded in the next run
    let status = response.status();
    if uri.is_file() || status.is_excluded() || status.is_unsupported() || status.is_unknown() {
        return response;
    }

    cache.insert(uri, status.into());
    response
}

fn show_progress(
    output: &mut dyn Write,
    progress_bar: &Option<ProgressBar>,
    response: &Response,
    formatter: &Arc<Box<dyn ResponseFormatter>>,
    verbose: &Verbosity,
) -> Result<()> {
    let out = formatter.write_response(response)?;
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
        .fail_map
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
    use log::info;

    use lychee_lib::{CacheStatus, ClientBuilder, InputSource, ResponseBody, Uri};

    use crate::formatters;

    use super::*;

    #[test]
    fn test_skip_cached_responses_in_progress_output() {
        let mut buf = Vec::new();
        let response = Response(
            InputSource::Stdin,
            ResponseBody {
                uri: Uri::try_from("http://127.0.0.1").unwrap(),
                status: Status::Cached(CacheStatus::Ok(200)),
            },
        );
        let formatter: Arc<Box<dyn ResponseFormatter>> =
            Arc::new(Box::new(formatters::response::Raw::new()));
        show_progress(
            &mut buf,
            &None,
            &response,
            &formatter,
            &Verbosity::default(),
        )
        .unwrap();

        info!("{:?}", String::from_utf8_lossy(&buf));
        assert!(buf.is_empty());
    }

    #[test]
    fn test_show_cached_responses_in_progress_debug_output() {
        let mut buf = Vec::new();
        let response = Response(
            InputSource::Stdin,
            ResponseBody {
                uri: Uri::try_from("http://127.0.0.1").unwrap(),
                status: Status::Cached(CacheStatus::Ok(200)),
            },
        );
        let formatter: Arc<Box<dyn ResponseFormatter>> =
            Arc::new(Box::new(formatters::response::Raw::new()));
        show_progress(&mut buf, &None, &response, &formatter, &Verbosity::debug()).unwrap();

        assert!(!buf.is_empty());
        let buf = String::from_utf8_lossy(&buf);
        assert_eq!(buf, "↻ [200] http://127.0.0.1/ | Cached: OK (cached)\n");
    }

    #[tokio::test]
    async fn test_invalid_url() {
        // Run a normal request with an invalid Url
        let client = ClientBuilder::builder().build().client().unwrap();
        let request = Request::try_from("http://\"").unwrap();
        let response = check_url(&client, request).await;
        assert!(response.status().is_error());
        assert!(matches!(
            response.status(),
            Status::Error(ErrorKind::InvalidURI(_))
        ));
    }
}
