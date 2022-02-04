use std::collections::HashSet;
use std::sync::Arc;

use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use lychee_lib::Collector;
use lychee_lib::ErrorKind;
use lychee_lib::Input;
use lychee_lib::InputSource;
use lychee_lib::Result;
use lychee_lib::Status;
use parking_lot::RwLock;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio_stream::StreamExt;

use crate::{
    cache::Cache,
    options::Config,
    stats::{color_response, ResponseStats},
    ExitCode,
};
use lychee_lib::{Client, Request, Response};

const SPINNER_CONFIG: &str =
    "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}";

/// A config object holding the state of the check run.
/// This is passed around to async functions recursively,
/// so it is easier than creating a `Check` struct with methods
struct CheckState {
    client: Client,
    cache: Arc<Cache>,
    cfg: Config,
    input_sources: HashSet<InputSource>,
    pb: Option<ProgressBar>,
    stats: ResponseStats,
}

#[allow(clippy::similar_names)]
pub(crate) async fn check(
    client: Client,
    cache: Arc<Cache>,
    inputs: Vec<Input>,
    cfg: Config,
) -> Result<(ResponseStats, Arc<Cache>, ExitCode)> {
    // Keep track of the original input sources for recursion
    let input_sources: HashSet<InputSource> = inputs.iter().map(|i| i.source.clone()).collect();

    let stats = ResponseStats::new();
    let pb = init_progress(cfg.no_progress);
    let semaphore = Arc::new(Semaphore::new(cfg.max_concurrency));

    let state = Arc::new(RwLock::new(CheckState {
        client,
        cache,
        cfg,
        input_sources,
        pb: pb.clone(),
        stats,
    }));

    let state_handle = state.clone();
    tokio::spawn(async move {
        handle_inputs(inputs, state_handle, semaphore).await;
    })
    .await?;

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    let code = if state.read().stats.is_success() {
        ExitCode::Success
    } else {
        ExitCode::LinkCheckFailure
    };

    let state = match Arc::try_unwrap(state) {
        Ok(state) => state,
        Err(_) => return Err(ErrorKind::Unlock),
    };
    let state = state.into_inner();
    Ok((state.stats, state.cache, code))
}

/// Extract the requests from the given inputs and handle each request concurrently
async fn handle_inputs(
    inputs: Vec<Input>,
    state: Arc<RwLock<CheckState>>,
    semaphore: Arc<Semaphore>,
) {
    let mut requests = Collector::from_iter(
        state.read().cfg.base.clone(),
        state.read().cfg.skip_missing,
        inputs,
    )
    .await;
    while let Some(request) = requests.next().await {
        let request = match request {
            Ok(request) => request,
            Err(_) => return,
        };
        if let Err(e) = handle_with_recursion(request, state.clone(), semaphore.clone()).await {
            println!("Error while handling request: {e}");
        };
    }
}

/// Handle a single request with optional recursion for subrequests
async fn handle_with_recursion(
    request: Request,
    state: Arc<RwLock<CheckState>>,
    semaphore: Arc<Semaphore>,
) -> Result<()> {
    let pb = &state.write().pb;
    if let Some(pb) = pb {
        pb.inc_length(1);
        pb.set_message(request.to_string());
    };
    let response = handle(request, state.clone()).await?;
    update_progress(pb, &response, state.read().cfg.verbose);

    if state.read().cfg.recursive {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Cannot acquire semaphore to handle request");
        recurse(permit, &response, state.clone(), semaphore);
    }
    state.write().stats.add(response);
    Ok(())
}

/// Initiate subrequests by creating a new input and sending it to the
/// the request collector for extraction
fn recurse(
    permit: OwnedSemaphorePermit,
    response: &Response,
    state: Arc<RwLock<CheckState>>,
    semaphore: Arc<Semaphore>,
) {
    let recursion_level = response.recursion_level() + 1;
    let depth = state.read().cfg.depth;

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
    if !should_recurse(&state.read().input_sources, source) {
        return;
    }

    // Construct the new recursive input
    let input = Input::with_recursion(&response.uri().to_string(), None, false, recursion_level);
    tokio::spawn(async move {
        handle_inputs(vec![input], state.clone(), semaphore).await;
        drop(permit);
    });
}

/// Handle a single request
async fn handle(request: Request, state: Arc<RwLock<CheckState>>) -> Result<Response> {
    let uri = request.uri.clone();
    if let Some(v) = state.read().cache.get(&uri) {
        // Found a cached request. Overwrite cache status in case the URI is
        // excluded in the current run
        let status = if state.read().client.is_excluded(&uri) {
            Status::Excluded
        } else {
            Status::from(v.value().status)
        };
        return Ok(Response::new(uri.clone(), status, request.source));
    }

    // Request was not cached; run a normal check
    // This can panic when the Url could not be parsed to a Uri.
    // See https://github.com/servo/rust-url/issues/554
    // See https://github.com/seanmonstar/reqwest/issues/668
    let response = state.read().client.check(request).await?;

    // Never cache filesystem access as it is fast already so caching has no
    // benefit
    if !uri.is_file() {
        state.write().cache.insert(uri, response.status().into());
    }
    Ok(response)
}

/// Initialize the interactive progress-bar if not disabled
fn init_progress(no_progress: bool) -> Option<ProgressBar> {
    if no_progress {
        None
    } else {
        let bar = ProgressBar::new_spinner()
            .with_style(ProgressStyle::default_bar().template(SPINNER_CONFIG));
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
                if url.domain().is_some() {
                    return true;
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
