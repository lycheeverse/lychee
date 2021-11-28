use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lychee_lib::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::{
    options::Config,
    stats::{color_response, ResponseStats},
    ExitCode,
};
use lychee_lib::{Client, Request, Response};

pub(crate) async fn check<S>(
    client: Client,
    requests: S,
    cfg: &Config,
) -> Result<(ResponseStats, ExitCode)>
where
    S: futures::Stream<Item = Request>,
{
    let (send_req, recv_req) = mpsc::channel(cfg.max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(cfg.max_concurrency);
    let max_concurrency = cfg.max_concurrency;
    let mut stats = ResponseStats::new();

    // Start receiving requests
    tokio::spawn(async move {
        futures::StreamExt::for_each_concurrent(
            ReceiverStream::new(recv_req),
            max_concurrency,
            |request| async {
                let response = client.check(request).await.expect("cannot check request");
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
        let request = request;
        if let Some(pb) = &bar {
            pb.inc_length(1);
            pb.set_message(&request.to_string());
        };
        send_req.send(request).await.expect("Cannot send request");
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
    Ok((stats, code))
}

fn show_progress(progress_bar: &Option<ProgressBar>, response: &Response, verbose: bool) {
    let out = color_response(&response.1);
    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(&out);
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
