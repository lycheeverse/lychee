#[macro_use]
extern crate log;

use anyhow::{anyhow, Result};
use headers::authorization::Basic;
use headers::{Authorization, HeaderMap, HeaderMapExt, HeaderName};
use indicatif::{ProgressBar, ProgressStyle};
use regex::RegexSet;
use std::str::FromStr;
use std::{collections::HashSet, time::Duration};
use structopt::StructOpt;

mod checker;
mod collector;
mod extract;
mod options;
mod stats;
mod types;
mod worker;

use checker::CheckerBuilder;
use options::{Config, LycheeOptions};
use stats::Stats;
use types::Response;
use types::{Excludes, Status};
use worker::Worker;

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
    pretty_env_logger::init();
    let opts = LycheeOptions::from_args();

    // Load a potentially existing config file and merge it into the config from the CLI
    let cfg = if let Some(c) = Config::load_from_file(&opts.config_file)? {
        opts.config.merge(c)
    } else {
        opts.config
    };

    let mut runtime = match cfg.threads {
        Some(threads) => {
            // We define our own runtime instead of the `tokio::main` attribute since we want to make the number of threads configurable
            tokio::runtime::Builder::new()
                .threaded_scheduler()
                .core_threads(threads)
                .enable_all()
                .build()?
        }
        None => tokio::runtime::Runtime::new()?,
    };
    let errorcode = runtime.block_on(run(cfg, opts.inputs))?;
    std::process::exit(errorcode);
}

async fn run(cfg: Config, inputs: Vec<String>) -> Result<i32> {
    let includes = RegexSet::new(&cfg.include)?;
    let excludes = Excludes::from_options(&cfg);
    let mut headers = parse_headers(cfg.headers)?;

    if let Some(auth) = cfg.basic_auth {
        let auth_header = parse_basic_auth(&auth)?;
        headers.typed_insert(auth_header);
    }

    let accepted = match cfg.accept {
        Some(accept) => Some(parse_statuscodes(accept)?),
        None => None,
    };
    let timeout = parse_timeout(cfg.timeout)?;
    let links = collector::collect_links(inputs, cfg.base_url.clone()).await?;
    let progress_bar = if cfg.progress {
        Some(
            ProgressBar::new(links.len() as u64)
            .with_style(
                ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {wide_msg}")
                .progress_chars("#>-")
            )
        )
    } else {
        None
    };

    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;
    let mut checker = CheckerBuilder::default();
    checker
        .includes(includes)
        .excludes(excludes)
        .max_redirects(cfg.max_redirects)
        .user_agent(cfg.user_agent.clone())
        .allow_insecure(cfg.insecure.clone())
        .custom_headers(headers)
        .method(method)
        .timeout(timeout.clone())
        .verbose(cfg.verbose.clone());

    if let Some(github_token) = cfg.github_token {
        checker.github_token(github_token);
    }
    if let Some(scheme) = cfg.scheme {
        checker.scheme(scheme);
    }
    if let Some(accepted) = accepted {
        checker.accepted(accepted);
    }

    let (send_req, recv_req) = async_channel::unbounded();
    let (send_resp, recv_resp) = async_channel::unbounded();

    let checker = checker.build()?;
    let mut worker = Worker::new(recv_req, send_resp, checker);
    let mut stats = Stats::new();

    tokio::spawn(async move {
        worker.listen().await;
    });

    let count = links.len();
    for link in links {
        if let Some(pb) = &progress_bar {
            pb.set_message(&link.to_string());
        };
        send_req.send(link).await?
    }

    for _ in 0..count {
        let r = recv_resp.recv().await?;
        if let Some(pb) = &progress_bar {
            pb.inc(1);
            // regular println! interferes with progress bar
            if let Some(message) = status_message(cfg.verbose, &r) {
                pb.println(message);
            }
        } else if let Some(message) = status_message(cfg.verbose, &r) {
            println!("{}", message);
        };
        stats.add(r);
    }

    // note that prints may interfere progress bar so this must go before summary
    if let Some(progress_bar) = progress_bar {
        progress_bar.finish_and_clear();
    }

    if cfg.verbose {
        stats.summary();
    }

    match stats.is_success() {
        true => Ok(ExitCode::Success as i32),
        false => Ok(ExitCode::LinkCheckFailure as i32),
    }
}

fn read_header(input: String) -> Result<(String, String)> {
    let elements: Vec<_> = input.split('=').collect();
    if elements.len() != 2 {
        return Err(anyhow!(
            "Header value should be of the form key=value, got {}",
            input
        ));
    }
    Ok((elements[0].into(), elements[1].into()))
}

fn parse_timeout(timeout: String) -> Result<Duration> {
    Ok(Duration::from_secs(timeout.parse::<u64>()?))
}

fn parse_headers(headers: Vec<String>) -> Result<HeaderMap> {
    let mut out = HeaderMap::new();
    for header in headers {
        let (key, val) = read_header(header)?;
        out.insert(
            HeaderName::from_bytes(key.as_bytes())?,
            val.parse().unwrap(),
        );
    }
    Ok(out)
}

fn parse_statuscodes(accept: String) -> Result<HashSet<http::StatusCode>> {
    let mut statuscodes = HashSet::new();
    for code in accept.split(',').into_iter() {
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

fn status_message(verbose: bool, response: &Response) -> Option<String> {
    match &response.status {
        Status::Ok(code) => {
            if verbose {
                Some(format!("✅{} [{}]", response.uri, code))
            } else {
                None
            }
        }
        Status::Failed(code) => Some(format!("🚫{} [{}]", response.uri, code)),
        Status::Redirected => {
            if verbose {
                Some(format!("🔀️{}", response.uri))
            } else {
                None
            }
        }
        Status::Excluded => {
            if verbose {
                Some(format!("👻{}", response.uri))
            } else {
                None
            }
        }
        Status::Error(e) => Some(format!("⚡ {} ({})", response.uri, e)),
        Status::Timeout => Some(format!("⌛{}", response.uri)),
    }
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
        assert_eq!(
            parse_headers(vec!["accept=text/html".into()]).unwrap(),
            custom
        );
    }

    #[test]
    fn test_parse_statuscodes() {
        let actual = parse_statuscodes("200,204,301".into()).unwrap();
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
