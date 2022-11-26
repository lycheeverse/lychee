use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lychee_lib::Status;
use lychee_lib::{ClientWrapper, Result};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::formatters::response::ResponseFormatter;
use crate::{cache::Cache, stats::ResponseStats, ExitCode};
use lychee_lib::{Request, Response};

use super::CommandParams;

pub(crate) async fn check<S>(
    params: CommandParams<S>,
) -> Result<(ResponseStats, Arc<Cache>, ExitCode)>
where
    S: futures::Stream<Item = Result<Request>>,
{
    let max_concurrency = params.cfg.max_concurrency;
    let mut stats = ResponseStats::new();
    let cache_ref = params.cache.clone();
    let client = Arc::new(Mutex::new(params.client));

    let cache = params.cache;

    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(max_concurrency);

    // Start receiving requests
    tokio::spawn(async move {
        futures::StreamExt::for_each_concurrent(
            ReceiverStream::new(recv_req),
            max_concurrency,
            |request: Result<Request>| async {
                let request = request.expect("cannot read request");
                let mut client = client.lock().await;
                let response = handle(&mut client, cache.clone(), request).await;

                send_resp
                    .send(response)
                    .await
                    .expect("cannot send response to queue");
            },
        )
        .await;
    });

    let pb = if params.cfg.no_progress {
        None
    } else {
        let bar = ProgressBar::new_spinner().with_style(ProgressStyle::default_bar().template(
            "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}",
        ).expect("Valid progress bar"));
        bar.set_length(0);
        bar.set_message("Extracting links");
        // report status _at least_ every 500ms
        bar.enable_steady_tick(Duration::from_millis(500));
        Some(bar)
    };

    let bar = pb.clone();

    let formatter = Arc::new(params.formatter);

    let show_results_task = tokio::spawn({
        let verbose = params.cfg.verbose;
        async move {
            while let Some(response) = recv_resp.recv().await {
                let response = response?;
                show_progress(&mut io::stdout(), &pb, &response, &formatter, verbose)?;
                stats.add(response);
            }
            Ok((pb, stats))
        }
    });

    let requests = params.requests;
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
    // required for the receiver task to end, which closes send_resp, which allows
    // the show_results_task to finish
    drop(send_req);

    let result: Result<(_, _)> = show_results_task.await?;
    let (pb, stats) = result?;

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    let code = if stats.is_success() {
        ExitCode::Success
    } else {
        ExitCode::LinkCheckFailure
    };
    Ok((stats, cache_ref, code))
}

/// Handle a single request
async fn handle(
    client: &mut ClientWrapper,
    cache: Arc<Cache>,
    request: Request,
) -> Result<Response> {
    let uri = request.uri.clone();
    if let Some(v) = cache.get(&uri) {
        // Found a cached request
        // Overwrite cache status in case the URI is excluded in the
        // current run
        let status = if client.is_excluded(&uri) {
            Status::Excluded
        } else {
            Status::from(v.value().status)
        };
        return Ok(Response::new(uri.clone(), status, request.source));
    }

    // Request was not cached; run a normal check
    //
    // This can panic when the Url could not be parsed to a Uri.
    // See https://github.com/servo/rust-url/issues/554
    // See https://github.com/seanmonstar/reqwest/issues/668
    // TODO: Handle error as soon as https://github.com/seanmonstar/reqwest/pull/1399 got merged
    let response = client.check(request).await.expect("cannot check URI");

    // - Never cache filesystem access as it is fast already so caching has no
    //   benefit.
    // - Skip caching unsupported URLs as they might be supported in a
    //   future run.
    // - Skip caching excluded links; they might not be excluded in the next run
    let status = response.status();
    if !uri.is_file() && !status.is_excluded() && !status.is_unsupported() {
        cache.insert(uri, status.into());
    }
    Ok(response)
}

fn show_progress(
    output: &mut dyn Write,
    progress_bar: &Option<ProgressBar>,
    response: &Response,
    formatter: &Arc<Box<dyn ResponseFormatter>>,
    verbose: bool,
) -> Result<()> {
    let out = formatter.write_response(response)?;
    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(out.clone());
        if verbose {
            pb.println(out);
        }
    } else if verbose || (!response.status().is_success() && !response.status().is_excluded()) {
        writeln!(output, "{}", out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use lychee_lib::{CacheStatus, InputSource, ResponseBody, Uri};

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
        show_progress(&mut buf, &None, &response, &formatter, false).unwrap();

        println!("{:?}", String::from_utf8_lossy(&buf));
        assert!(buf.is_empty());
    }
}
