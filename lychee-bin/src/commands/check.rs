use std::sync::Arc;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lychee_lib::Collector;
use lychee_lib::Input;
use lychee_lib::Result;
use lychee_lib::Status;
use parking_lot::RwLock;
use tokio::sync::mpsc;

use crate::{
    cache::Cache,
    options::Config,
    stats::{color_response, ResponseStats},
    ExitCode,
};
use lychee_lib::{Client, Request, Response};

pub(crate) async fn check(
    client: Client,
    cache: Arc<Cache>,
    inputs: Vec<Input>,
    cfg: Config,
) -> Result<(Arc<RwLock<ResponseStats>>, Arc<Cache>, ExitCode)> {
    let (send_input, recv_input) = mpsc::channel(cfg.max_concurrency);

    let base = cfg.base.clone();
    let verbose = cfg.verbose;
    let max_concurrency = cfg.max_concurrency;

    // Get handles for values that will be moved into the async closure
    let cache_handle = cache.clone();
    let stats = Arc::new(RwLock::new(ResponseStats::new()));
    let stats_handle = stats.clone();
    let pb = init_progress(cfg.no_progress);
    let pb_handle = pb.clone();

    let requests = Collector::new(base, cfg.skip_missing)
        .from_chan(recv_input)
        .await;

    // Start receiving requests
    let collector_handle = tokio::spawn(async move {
        futures::StreamExt::for_each_concurrent(
            requests,
            max_concurrency,
            |request: Result<Request>| async {
                let request = request.expect("cannot read request");
                if let Some(pb) = &pb {
                    pb.inc_length(1);
                    pb.set_message(request.to_string());
                };
                let response = handle_request(&client, cache.clone(), request).await;
                update_progress(&pb, &response, verbose);
                stats.write().add(response);
            },
        )
        .await;
    });

    for input in inputs {
        send_input
            .send(input)
            .await
            .expect("Cannot send input to channel");
    }
    // Explicitly drop the channel to stop the stream inside the collector
    drop(send_input);

    collector_handle.await?;

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb_handle {
        pb.finish_and_clear();
    }

    let code = if stats_handle.read().is_success() {
        ExitCode::Success
    } else {
        ExitCode::LinkCheckFailure
    };
    Ok((stats_handle, cache_handle, code))
}

/// Handle a single request
async fn handle_request(client: &Client, cache: Arc<Cache>, request: Request) -> Response {
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

/// Initialize the interactive progress-bar if not disabled
fn init_progress(no_progress: bool) -> Option<ProgressBar> {
    if no_progress {
        None
    } else {
        let bar = ProgressBar::new_spinner().with_style(ProgressStyle::default_bar().template(
            "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}",
        ));
        bar.set_length(0);
        bar.set_message("Extracting links");
        bar.enable_steady_tick(100);
        Some(bar)
    }
}

/// Update the progress on every new response
fn update_progress(progress_bar: &Option<ProgressBar>, response: &Response, verbose: bool) {
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
