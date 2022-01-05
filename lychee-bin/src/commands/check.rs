use std::sync::Arc;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lychee_lib::Result;
use lychee_lib::Status;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::{
    cache::Cache,
    options::Config,
    stats::{color_response, ResponseStats},
    ExitCode,
};
use lychee_lib::{Client, Request, Response};

pub(crate) async fn check<S>(
    client: Client,
    cache: Arc<Cache>,
    requests: S,
    cfg: &Config,
) -> Result<(ResponseStats, Arc<Cache>, ExitCode)>
where
    S: futures::Stream<Item = Result<Request>>,
{
    let (send_req, recv_req) = mpsc::channel(cfg.max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(cfg.max_concurrency);
    let max_concurrency = cfg.max_concurrency;
    let mut stats = ResponseStats::new();
    let cache_ref = cache.clone();

    // Start receiving requests
    tokio::spawn(async move {
        futures::StreamExt::for_each_concurrent(
            ReceiverStream::new(recv_req),
            max_concurrency,
            |request: Result<Request>| async {
                let request = request.expect("cannot read request");
                let response = handle(&client, cache.clone(), request).await;

                send_resp
                    .send(response)
                    .await
                    .expect("cannot send response to queue");
            },
        )
        .await;
    });

    let pb = if cfg.no_progress {
        None
    } else {
        let bar = ProgressBar::new_spinner().with_style(ProgressStyle::default_bar().template(
            "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}",
        ));
        bar.set_length(0);
        bar.set_message("Extracting links");
        bar.enable_steady_tick(100);
        Some(bar)
    };

    let bar = pb.clone();
    let show_results_task = tokio::spawn({
        let verbose = cfg.verbose;
        async move {
            while let Some(response) = recv_resp.recv().await {
                show_progress(&pb, &response, verbose);
                stats.add(response);
            }
            (pb, stats)
        }
    });

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

    let (pb, stats) = show_results_task.await?;

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
async fn handle(client: &Client, cache: Arc<Cache>, request: Request) -> Response {
    let uri = request.uri.clone();
    if let Some(v) = cache.get(&uri) {
        // Found a cached request
        // Overwrite cache status in case the URI is excluded in the
        // current run
        let status = if client.is_excluded(&uri) {
            Status::Excluded
        } else {
            Status::from(*v.value())
        };
        return Response::new(uri.clone(), status, request.source);
    }

    // Request was not cached; run a normal check
    // This can panic when the Url could not be parsed to a Uri.
    // See https://github.com/servo/rust-url/issues/554
    // See https://github.com/seanmonstar/reqwest/issues/668
    // TODO: Handle error as soon as https://github.com/seanmonstar/reqwest/pull/1399 got merged
    let response = client.check(request).await.expect("cannot check URI");

    // Never cache filesystem access as it is fast already so caching has no
    // benefit
    if !uri.is_file() {
        cache.insert(uri, response.status().into());
    }
    response
}

fn show_progress(progress_bar: &Option<ProgressBar>, response: &Response, verbose: bool) {
    let out = color_response(&response.1);
    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(out.clone());
        if verbose {
            pb.println(out);
        }
    } else {
        if (response.status().is_success() || response.status().is_excluded()) && !verbose {
            return;
        }
        println!("{}", out);
    }
}
