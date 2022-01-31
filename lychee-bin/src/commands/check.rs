use std::collections::HashSet;
use std::sync::Arc;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lychee_lib::Collector;
use lychee_lib::Input;
use lychee_lib::InputSource;
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
    let base = cfg.base.clone();
    let verbose = cfg.verbose;
    let max_concurrency = cfg.max_concurrency;

    let input_sources: HashSet<InputSource> = inputs.iter().map(|i| i.source.clone()).collect();

    // Get handles for values that will be moved into the async closure
    let cache_handle = cache.clone();
    let stats = Arc::new(RwLock::new(ResponseStats::new()));
    let stats_handle = stats.clone();
    let pb = init_progress(cfg.no_progress);
    let pb_handle = pb.clone();

    let (sender, requests) =
        Collector::from_chan(base, cfg.skip_missing, cfg.max_concurrency).await;

    for input in inputs {
        sender
            .send(input)
            .await
            .expect("Cannot send input to channel");
    }

    // Explicitly drop the channel to stop the stream inside the collector
    // Start receiving requests
    let collector_handle = tokio::spawn(async move {
        futures::StreamExt::for_each_concurrent(
            requests,
            max_concurrency,
            |request: Result<Request>| async {
                let request = match request {
                    Ok(request) => request,
                    Err(_) => return,
                };
                if let Some(pb) = &pb {
                    pb.inc_length(1);
                    pb.set_message(request.to_string());
                };
                let response = handle_request(&client, cache.clone(), request).await;
                update_progress(&pb, &response, verbose);

                if cfg.recursive {
                    recurse(&response, cfg.depth, &input_sources, &sender.clone()).await;
                }
                stats.write().add(response);
            },
        )
        .await;
        drop(sender);
    });

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

/// Traverse children of response by creating a new input and sending it to the
/// channel of the collector
async fn recurse(
    response: &Response,
    depth: Option<isize>,
    input_sources: &HashSet<InputSource>,
    sender: &mpsc::Sender<Input>,
) {
    let recursion_level = response.recursion_level() + 1;

    if let Some(depth) = depth {
        if depth != -1 && recursion_level > depth {
            // Maximum recursion depth reached;
            // stop link checking.
            return;
        }
    }

    if !response.is_success() {
        // Don't recurse if the previous request was not
        // successful
        return;
    }

    // Check if this URI was checked before, and can be
    // skipped
    if response.is_cached() {
        return;
    }

    // Only domains, which were part of the original
    // input should be checked recursively
    let source = response.source();
    if !should_recurse(input_sources, source) {
        return;
    }

    // Construct the new recursive input
    let input = Input::with_recursion(&response.uri().to_string(), None, false, recursion_level);
    sender
        .send(input)
        .await
        .expect("Can't send recursive input to channel");
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

/// Check if the given source is part of the original set of inputs
/// This is needed to limit recursion to known resources
fn should_recurse(inputs: &HashSet<InputSource>, source: &InputSource) -> bool {
    if matches!(
        source,
        InputSource::Stdin | InputSource::String(_) | InputSource::FsGlob { .. }
    ) {
        // Don't recurse
        return false;
    }
    for input in inputs {
        match input {
            InputSource::RemoteUrl(url) => {
                if let Some(domain) = url.domain() {
                    if Some(domain) == url.domain() {
                        return true;
                    }
                }
            }
            InputSource::FsPath(_path) => {
                // TODO: Add support for file recursion
                //
                // cwd: /path/to/pub/
                // checking: some/other/file.txt
                //
                // path: foo
                // resolved: /path/to/pub/some/other/foo
                //
                // path: /blub
                // resolved: /pat/to/pub/blub
                //
                // path: ../foo
                // resolved: /path/to/pub/some/foo
                //
                // path: ./foo/bar
                // resolved: /path/to/pub/some/other/foo/bar
            }
            _ => (),
        };
    }
    false
}
