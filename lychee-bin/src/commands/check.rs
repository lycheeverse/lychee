use std::collections::HashSet;
use std::pin::pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::stream;
use futures::{Stream, StreamExt, future::Either};
use log::warn;
use reqwest::Url;
use tokio::sync::mpsc;

use lychee_lib::InputSource;
use lychee_lib::RequestError;
use lychee_lib::Status;
use lychee_lib::archive::Archive;
use lychee_lib::async_lib::stream::StreamExt as _;
use lychee_lib::async_lib::waiter::{WaitGroup, WaitGuard};
use lychee_lib::ratelimit::HostPool;
use lychee_lib::{Client, Collector, ErrorKind, FileExtensions, Input, Request, Response};
use tokio_stream::wrappers::ReceiverStream;

use crate::CommandParams;
use crate::config::Config;
use crate::formatters::stats::ResponseStats;
use crate::formatters::suggestion::Suggestion;
use crate::progress::Progress;
use crate::{ExitCode, cache::Cache};

type RecursiveRequest = (WaitGuard, Request, usize);

#[derive(Clone)]
struct Recursion {
    enabled: bool,
    max_depth: Option<usize>,
    domains: Arc<HashSet<String>>,
    crawled_urls: Arc<Mutex<HashSet<Url>>>,
    collector: Collector,
    extensions: FileExtensions,
}

impl Recursion {
    fn new(cfg: &Config, collector: Collector, mut input_domains: HashSet<String>) -> Self {
        input_domains.extend(
            cfg.recursed_domains()
                .iter()
                .filter_map(|domain| normalize_domain(domain)),
        );
        Self {
            enabled: cfg.recursive(),
            max_depth: cfg.max_depth(),
            domains: Arc::new(input_domains),
            crawled_urls: Arc::new(Mutex::new(HashSet::new())),
            collector,
            extensions: cfg.extensions(),
        }
    }

    fn target(&self, response: &Response, depth: usize) -> Option<Url> {
        if !self.enabled
            || self.max_depth.is_some_and(|limit| depth >= limit)
            || !response.status().is_success()
        {
            return None;
        }

        if let InputSource::RemoteUrl(source) = response.source() {
            self.crawled_urls
                .lock()
                .unwrap()
                .insert(canonical_url(source));
        }

        let url = response.redirects().map_or_else(
            || Url::try_from(response.body().uri.as_str()).ok(),
            |redirects| Some(redirects.destination().clone()),
        )?;
        let url = canonical_url(&url);
        can_recurse_into(&url, &self.domains).then_some(url)
    }

    async fn requests_for(
        &self,
        url: Option<Url>,
        guard: WaitGuard,
        depth: usize,
    ) -> Vec<RecursiveRequest> {
        let Some(url) = url else {
            return Vec::new();
        };

        if !self.crawled_urls.lock().unwrap().insert(url.clone()) {
            return Vec::new();
        }

        collect_recursive_requests(self.collector.clone(), url, self.extensions.clone())
            .await
            .into_iter()
            .map(|request| (guard.clone(), request, depth + 1))
            .collect()
    }
}

#[allow(clippy::match_bool, reason = "more readable and compact")]
#[allow(
    clippy::result_large_err,
    reason = "no point in using Box<Response> inside Err when we have whole streams \
              of Response in other places. also, streams are lazy and on-demand."
)]
pub(crate) async fn check(
    params: CommandParams<impl Stream<Item = Result<Request, RequestError>>>,
) -> Result<(ResponseStats, Cache, ExitCode, Arc<HostPool>), ErrorKind> {
    let CommandParams {
        client,
        cache,
        collector,
        requests,
        recursion_domains,
        cfg,
        is_stdin_input,
    } = params;

    /* Config options, progress bar, and stats */

    let max_concurrency = cfg.max_concurrency().get();

    let level = cfg.verbose().log_level();
    let hide_bar = cfg.no_progress() || is_stdin_input;

    let accept = cfg.accept().into();
    let cache_exclude_status = cfg.cache_exclude_status().into();
    let recursion = Recursion::new(&cfg, collector, recursion_domains);

    let progress = Progress::new("Extracting links", hide_bar, level, &cfg.mode());

    let mut stats = match cfg.verbose().log_level() >= log::Level::Info {
        true => ResponseStats::extended(),
        false => ResponseStats::default(),
    };

    /* Input streams and channels (both initial and recursive) */

    let (waiter, wait_guard) = WaitGroup::new();

    // Split initial requests into: valid requests and request errors. Note that
    // this stream closure *owns* a wait guard, so we must drop the closure after
    // it's finished to avoid deadlock. This is done using the `.chain()` combinator.
    let (valid_requests, request_errors) = requests
        .inspect(|_| progress.inc_length(1))
        .map(move |request| (request, wait_guard.clone()))
        .chain(futures::stream::empty())
        .map(|(request, guard)| match request {
            Ok(request) => Ok((guard, request, 0)),
            Err(request_error) => Err((guard, request_error)),
        })
        .partition_result::<(WaitGuard, Request, usize), (WaitGuard, RequestError)>();

    // Further partition the request errors into request building errors (like
    // unresolved relative URLs) and fatal errors when fetching a user input fails.
    let (request_building_errors, mut fatal_errors) = request_errors
        .map(
            |(guard, request_error)| match request_error.into_response() {
                Ok(request_building_error) => Ok((guard, request_building_error)),
                Err(fatal_user_input_error) => Err((guard, fatal_user_input_error)),
            },
        )
        .partition_result::<(WaitGuard, Response), (WaitGuard, ErrorKind)>();
    let request_building_errors =
        request_building_errors.map(|(guard, response)| (guard, response, 0));

    let (recursive_channel_send, recursive_channel_recv) = mpsc::channel(max_concurrency);

    let send_recursive_req = |request| {
        let progress = progress.clone();
        let recursive_channel_send = recursive_channel_send.clone();
        async move { send_recursive_request(recursive_channel_send, &progress, request) }
    };

    // Combine recursive requests and input requests.
    let requests = futures::stream::select_with_strategy(
        valid_requests,
        ReceiverStream::new(recursive_channel_recv).take_until(waiter.wait()),
        |()| futures::stream::PollNext::Right, // Recursive requests consume memory, prefer those.
    );

    /* Main link checking pipeline */

    // Perform requests. This is the only part of the main pipeline that happens concurrently.
    let check_responses = requests
        .map(
            async |(guard, request, depth)| -> (WaitGuard, Response, usize) {
                let check_url = |r| check_url(&client, r);
                // TODO: eventually, this should be a checker that uses a cache, rather than
                // a cache that uses a checker.
                let response = cache
                    .handle(&client, &cache_exclude_status, &accept, request, check_url)
                    .await;
                (guard, response, depth)
            },
        )
        .buffer_unordered(max_concurrency);

    let responses = futures::stream::select(check_responses, request_building_errors);

    // Increment stats and extract recursive uris from responses.
    let recursive_uris = responses
        .map(|(guard, response, depth)| {
            progress.update(Some(response.body()));
            let recursive_url = recursion.target(&response, depth);
            stats.add(response);

            let recursion = recursion.clone();
            async move { recursion.requests_for(recursive_url, guard, depth).await }
        })
        .buffer_unordered(max_concurrency);

    // Send recursive uris back to the initial channel. This will terminate
    // only when all requests are finished and all `WaitGuard`s are dropped.
    let all_done = recursive_uris
        .flat_map(stream::iter)
        .for_each(send_recursive_req);

    let start = std::time::Instant::now();

    /* Setup complete and streams ready. Starting execution */

    // This `await` is where execution begins. All streams are polled concurrently
    // and we wait for `all_done` or an early return with an error value.
    // WARNING: Before changing the `.await` structure, be aware of the
    // requirements imposed by [`lychee_lib::async_lib::stream::partition_result`].
    // Partitioned streams must be concurrently polled. At the moment, this is
    // achieved because all partitioned streams are within `all_done`.
    match futures::future::select(pin!(all_done), fatal_errors.next()).await {
        Either::Left(((), _fatal_errors)) => (),
        Either::Right((None, remaining)) => remaining.await,
        Either::Right((Some((_guard, fatal_error)), _remaining)) => {
            progress.finish("Error while fetching initial inputs");
            return Err(fatal_error);
        }
    }

    /* Main execution finished. Finalise stats and archive suggestions */

    progress.finish("Finished processing links");
    stats.duration = start.elapsed();

    if cfg.suggest() {
        let progress = Progress::new("Searching for alternatives", hide_bar, level, &cfg.mode());
        let archive = cfg.archive();
        let timeout = cfg.timeout();
        suggest_archived_links(archive, &mut stats, progress, max_concurrency, timeout).await;
    }

    let code = exit_code(&cfg, &stats);
    Ok((stats, cache, code, client.host_pool()))
}

fn exit_code(cfg: &Config, stats: &ResponseStats) -> ExitCode {
    let is_success = if cfg.accept_timeouts() {
        stats.is_success_ignoring_timeouts()
    } else {
        stats.is_success()
    };
    if is_success {
        ExitCode::Success
    } else {
        ExitCode::LinkCheckFailure
    }
}

fn send_recursive_request(
    sender: mpsc::Sender<RecursiveRequest>,
    progress: &Progress,
    request: RecursiveRequest,
) {
    progress.inc_length(1);
    // Sending must happen independently from response processing. Otherwise a
    // full bounded channel would stop us from polling its receiver and deadlock.
    tokio::spawn(async move {
        sender.send(request).await.unwrap_or_else(|e| {
            warn!("unable to send recursive uri {:?} - channel closed?", e.0);
        });
    });
}

fn canonical_url(url: &Url) -> Url {
    let mut url = url.clone();
    url.set_fragment(None);
    url
}

fn normalize_domain(domain: &str) -> Option<String> {
    let domain = domain.trim();
    if domain.is_empty() {
        return None;
    }

    let url = if domain.contains("://") {
        Url::parse(domain).ok()?
    } else {
        Url::parse(&format!("http://{domain}")).ok()?
    };

    url.host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

fn can_recurse_into(url: &Url, domains: &HashSet<String>) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }

    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();

    domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

async fn collect_recursive_requests(
    collector: Collector,
    url: Url,
    extensions: FileExtensions,
) -> Vec<Request> {
    let input = Input::from_input_source(InputSource::RemoteUrl(Box::new(url.clone())));
    let results = collector
        .collect_links_from_file_types(HashSet::from([input]), extensions)
        .collect::<Vec<_>>()
        .await;

    results
        .into_iter()
        .filter_map(|result| match result {
            Ok(request) => Some(request),
            Err(error) => {
                warn!("unable to collect links recursively from {url}: {error}");
                None
            }
        })
        .collect()
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
        let request = Request::try_from("https://wikipedia.org/").unwrap();
        let response = cache
            .handle(
                &client,
                &StatusCodeSelector::empty().into(),
                &StatusCodeSelector::default_accepted().into(),
                request,
                |r| check_url(&client, r),
            )
            .await;
        assert!(response.status().is_error());
        assert!(cache.contains_key(&Uri::try_from("https://wikipedia.org/404").unwrap()));
    }
}
