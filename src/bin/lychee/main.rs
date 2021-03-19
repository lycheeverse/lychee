use anyhow::{anyhow, Context, Result};
use headers::authorization::Basic;
use headers::{Authorization, HeaderMap, HeaderMapExt, HeaderName};
use indicatif::{ProgressBar, ProgressStyle};
use options::Format;
use regex::RegexSet;
use stats::color_response;
use std::{collections::HashSet, time::Duration};
use std::{fs, str::FromStr};
use structopt::StructOpt;
use tokio::sync::mpsc::{self, Sender};

mod options;
mod stats;

use crate::options::{Config, LycheeOptions};
use crate::stats::ResponseStats;

use lychee::{
    collector::{self, Input},
    Cache, Request,
};
use lychee::{ClientBuilder, ClientPool, Response};

/// A C-like enum that can be cast to `i32` and used as process exit code.
enum ExitCode {
    Success = 0,
    // NOTE: exit code 1 is used for any `Result::Err` bubbled up to `main()` using the `?` operator.
    // For now, 1 acts as a catch-all for everything non-link related (including config errors),
    // until we find a way to structure the error code handling better.
    #[allow(unused)]
    UnexpectedFailure = 1,
    LinkCheckFailure = 2,
}

fn main() -> Result<()> {
    // std::process::exit doesn't guarantee that all destructors will be ran,
    // therefore we wrap "main" code in another function to guarantee that.
    // See: https://doc.rust-lang.org/stable/std/process/fn.exit.html
    // Also see: https://www.youtube.com/watch?v=zQC8T71Y8e4
    let exit_code = run_main()?;
    std::process::exit(exit_code);
}

fn run_main() -> Result<i32> {
    let mut opts = LycheeOptions::from_args();

    // Load a potentially existing config file and merge it into the config from the CLI
    if let Some(c) = Config::load_from_file(&opts.config_file)? {
        opts.config.merge(c)
    }
    let cfg = &opts.config;

    let runtime = match cfg.threads {
        Some(threads) => {
            // We define our own runtime instead of the `tokio::main` attribute
            // since we want to make the number of threads configurable
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads)
                .enable_all()
                .build()?
        }
        None => tokio::runtime::Runtime::new()?,
    };

    runtime.block_on(run(cfg, opts.inputs()))
}

fn show_progress(progress_bar: &Option<ProgressBar>, response: &Response, verbose: bool) {
    let out = color_response(response);
    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(&out);
        if verbose {
            pb.println(out);
        }
    } else {
        if (response.status.is_success() || response.status.is_excluded()) && !verbose {
            return;
        }
        println!("{}", out);
    }
}

fn fmt(stats: &ResponseStats, format: &Format) -> Result<String> {
    Ok(match format {
        Format::String => stats.to_string(),
        Format::Json => serde_json::to_string_pretty(&stats)?,
    })
}

// Get the set of input domains
// This is needed for supporting recursion
fn input_domains(inputs: Vec<Input>) -> HashSet<String> {
    let mut domains = HashSet::new();
    for input in inputs {
        if let Input::RemoteUrl(url) = input {
            if let Some(domain) = url.domain() {
                domains.insert(domain.to_string());
            }
        }
    }
    domains
}

async fn run(cfg: &Config, inputs: Vec<Input>) -> Result<i32> {
    let mut headers = parse_headers(&cfg.headers)?;
    if let Some(auth) = &cfg.basic_auth {
        let auth_header = parse_basic_auth(&auth)?;
        headers.typed_insert(auth_header);
    }

    let accepted = cfg.accept.clone().and_then(|a| parse_statuscodes(&a).ok());
    let timeout = parse_timeout(cfg.timeout);
    let max_concurrency = cfg.max_concurrency;
    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;
    let include = RegexSet::new(&cfg.include)?;
    let exclude = RegexSet::new(&cfg.exclude)?;

    let client = ClientBuilder::default()
        .includes(include)
        .excludes(exclude)
        .exclude_all_private(cfg.exclude_all_private)
        .exclude_private_ips(cfg.exclude_private)
        .exclude_link_local_ips(cfg.exclude_link_local)
        .exclude_loopback_ips(cfg.exclude_loopback)
        .max_redirects(cfg.max_redirects)
        .user_agent(cfg.user_agent.clone())
        .allow_insecure(cfg.insecure)
        .custom_headers(headers)
        .method(method)
        .timeout(timeout)
        .github_token(cfg.github_token.clone())
        .scheme(cfg.scheme.clone())
        .accepted(accepted)
        .build()?;

    // Create link cache to keep track of seen links
    let mut cache = Cache::new();

    let links = collector::collect_links(
        &inputs,
        cfg.base_url.clone(),
        cfg.skip_missing,
        max_concurrency,
    )
    .await?;
    let mut total_requests = links.len();

    let pb = match cfg.no_progress {
        true => None,
        false => {
            let bar = ProgressBar::new(links.len() as u64)
                .with_style(ProgressStyle::default_bar().template(
                "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25.magenta.bright/white} {wide_msg}",
            )
            .progress_chars("██"));
            bar.enable_steady_tick(100);
            Some(bar)
        }
    };

    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(max_concurrency);

    let mut stats = ResponseStats::new();

    let bar = pb.clone();
    let sr = send_req.clone();
    tokio::spawn(async move {
        for link in links {
            if let Some(pb) = &bar {
                pb.set_message(&link.to_string());
            };
            sr.send(link).await.unwrap();
        }
    });

    tokio::spawn(async move {
        // Start receiving requests
        let clients: Vec<_> = (0..max_concurrency).map(|_| client.clone()).collect();
        let mut clients = ClientPool::new(send_resp, recv_req, clients);
        clients.listen().await;
    });

    let input_domains: HashSet<String> = input_domains(inputs);

    // We keep track of the total number of requests
    // and exit the loop once we reach it.
    // Otherwise the sender would never be dropped and
    // we'd be stuck indefinitely.
    let mut curr = 0;

    while curr < total_requests {
        curr += 1;
        let response = recv_resp.recv().await.context("Receive channel closed")?;

        show_progress(&pb, &response, cfg.verbose);
        stats.add(response.clone());

        if cfg.recursive {
            let count = recurse(
                response,
                &mut cache,
                &input_domains,
                &cfg,
                &pb,
                send_req.clone(),
            )
            .await?;
            total_requests += count;
        }
    }

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    let stats_formatted = fmt(&stats, &cfg.format)?;
    if let Some(output) = &cfg.output {
        fs::write(output, stats_formatted).context("Cannot write status output to file")?;
    } else {
        if cfg.verbose && !stats.is_empty() {
            // separate summary from the verbose list of links above
            println!();
        }
        // we assume that the formatted stats don't have a final newline
        println!("{}", stats_formatted);
    }

    match stats.is_success() {
        true => Ok(ExitCode::Success as i32),
        false => Ok(ExitCode::LinkCheckFailure as i32),
    }
}

async fn recurse(
    response: Response,
    cache: &mut Cache,
    input_domains: &HashSet<String>,
    cfg: &Config,
    pb: &Option<ProgressBar>,
    send_req: Sender<Request>,
) -> Result<usize> {
    let recursion_level = response.recursion_level + 1;

    if let Some(depth) = cfg.depth {
        if recursion_level > depth {
            // Maximum recursion depth reached; stop link checking.
            return Ok(0);
        }
    }

    if !response.status.is_success() {
        return Ok(0);
    }
    if cache.contains(response.uri.as_str()) {
        return Ok(0);
    }
    cache.insert(response.uri.to_string());

    if let lychee::Uri::Website(url) = response.uri {
        let input = collector::Input::RemoteUrl(url.clone());

        // Check domain against known domains
        // If no domain is given, it might be a local link (e.g. 127.0.0.1),
        // which we accept
        if let Some(domain) = url.domain() {
            if !input_domains.contains(domain) {
                return Ok(0);
            }
        }

        let links = collector::collect_links(
            &[input],
            cfg.base_url.clone(),
            cfg.skip_missing,
            cfg.max_concurrency,
        )
        .await?;
        let count = links.len();

        let bar = pb.clone();
        tokio::spawn(async move {
            for mut link in links {
                link.recursion_level = recursion_level;
                if let Some(pb) = &bar {
                    pb.inc_length(1);
                    pb.set_message(&link.to_string());
                };
                send_req.send(link).await.unwrap();
            }
        });
        return Ok(count);
    };
    Ok(0)
}

fn read_header(input: &str) -> Result<(String, String)> {
    let elements: Vec<_> = input.split('=').collect();
    if elements.len() != 2 {
        return Err(anyhow!(
            "Header value should be of the form key=value, got {}",
            input
        ));
    }
    Ok((elements[0].into(), elements[1].into()))
}

fn parse_timeout(timeout: usize) -> Duration {
    Duration::from_secs(timeout as u64)
}

fn parse_headers<T: AsRef<str>>(headers: &[T]) -> Result<HeaderMap> {
    let mut out = HeaderMap::new();
    for header in headers {
        let (key, val) = read_header(header.as_ref())?;
        out.insert(
            HeaderName::from_bytes(key.as_bytes())?,
            val.parse().unwrap(),
        );
    }
    Ok(out)
}

fn parse_statuscodes<T: AsRef<str>>(accept: T) -> Result<HashSet<http::StatusCode>> {
    let mut statuscodes = HashSet::new();
    for code in accept.as_ref().split(',').into_iter() {
        let code: reqwest::StatusCode = reqwest::StatusCode::from_bytes(code.as_bytes())?;
        statuscodes.insert(code);
    }
    Ok(statuscodes)
}

fn parse_basic_auth(auth: &str) -> Result<Authorization<Basic>> {
    let params: Vec<_> = auth.split(':').collect();
    if params.len() != 2 {
        return Err(anyhow!(
            "Basic auth value should be of the form username:password, got {}",
            auth
        ));
    }
    Ok(Authorization::basic(params[0], params[1]))
}

#[cfg(test)]
mod test {
    use super::*;
    use http::StatusCode;
    use reqwest::header;

    #[test]
    fn test_parse_custom_headers() {
        let mut custom = HeaderMap::new();
        custom.insert(header::ACCEPT, "text/html".parse().unwrap());
        assert_eq!(parse_headers(&["accept=text/html"]).unwrap(), custom);
    }

    #[test]
    fn test_parse_statuscodes() {
        let actual = parse_statuscodes("200,204,301").unwrap();
        let expected: HashSet<StatusCode> = [
            StatusCode::OK,
            StatusCode::NO_CONTENT,
            StatusCode::MOVED_PERMANENTLY,
        ]
        .iter()
        .cloned()
        .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_basic_auth() {
        let mut expected = HeaderMap::new();
        expected.insert(
            header::AUTHORIZATION,
            "Basic YWxhZGluOmFicmV0ZXNlc2Ftbw==".parse().unwrap(),
        );

        let mut actual = HeaderMap::new();
        let auth_header = parse_basic_auth("aladin:abretesesamo").unwrap();
        actual.typed_insert(auth_header);
        assert_eq!(expected, actual);
    }
}
