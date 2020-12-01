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
use tokio::sync::mpsc;

mod client;
mod client_pool;
mod collector;
mod extract;
mod options;
mod stats;
mod types;

#[cfg(test)]
pub(crate) mod test_utils;

use client::ClientBuilder;
use client_pool::ClientPool;
use collector::Input;
use options::{Config, LycheeOptions};
use stats::ResponseStats;
use types::{Excludes, Response, Status};

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
    let mut opts = LycheeOptions::from_args();

    // Load a potentially existing config file and merge it into the config from the CLI
    if let Some(c) = Config::load_from_file(&opts.config_file)? {
        opts.config.merge(c)
    }
    let cfg = &opts.config;

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
    let errorcode = runtime.block_on(run(cfg, opts.inputs()))?;
    std::process::exit(errorcode);
}

fn show_progress(progress_bar: &Option<ProgressBar>, response: &Response, verbose: bool) {
    let message = status_message(&response, verbose);
    if let Some(pb) = progress_bar {
        pb.inc(1);
        // regular println! interferes with progress bar
        if let Some(message) = message {
            pb.println(message);
        }
    } else if let Some(message) = message {
        println!("{}", message);
    };
}

async fn run(cfg: &Config, inputs: Vec<Input>) -> Result<i32> {
    let mut headers = parse_headers(&cfg.headers)?;
    if let Some(auth) = &cfg.basic_auth {
        let auth_header = parse_basic_auth(&auth)?;
        headers.typed_insert(auth_header);
    }

    let accepted = cfg.accept.clone().and_then(|a| parse_statuscodes(&a).ok());
    let timeout = parse_timeout(&cfg.timeout)?;
    let max_concurrency = cfg.max_concurrency.parse()?;
    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;
    let includes = RegexSet::new(&cfg.include)?;
    let excludes = Excludes::from_options(&cfg);

    let client = ClientBuilder::default()
        .includes(includes)
        .excludes(excludes)
        .max_redirects(cfg.max_redirects)
        .user_agent(cfg.user_agent.clone())
        .allow_insecure(cfg.insecure)
        .custom_headers(headers)
        .method(method)
        .timeout(timeout)
        .verbose(cfg.verbose)
        .github_token(cfg.github_token.clone())
        .scheme(cfg.scheme.clone())
        .accepted(accepted)
        .build()?;

    let links = collector::collect_links(&inputs, cfg.base_url.clone(), cfg.skip_missing).await?;
    let pb = if cfg.progress {
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

    let (mut send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(max_concurrency);

    let mut stats = ResponseStats::new();

    let bar = pb.clone();
    tokio::spawn(async move {
        for link in links {
            if let Some(pb) = &bar {
                pb.set_message(&link.to_string());
            };
            send_req.send(link).await.unwrap();
        }
    });

    tokio::spawn(async move {
        // Start receiving requests
        let clients: Vec<_> = (0..max_concurrency).map(|_| client.clone()).collect();
        let mut clients = ClientPool::new(send_resp, recv_req, clients);
        clients.listen().await;
    });

    while let Some(response) = recv_resp.recv().await {
        show_progress(&pb, &response, cfg.verbose);
        stats.add(response);
    }

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    if cfg.verbose {
        println!("\n{}", stats);
    }

    match stats.is_success() {
        true => Ok(ExitCode::Success as i32),
        false => Ok(ExitCode::LinkCheckFailure as i32),
    }
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

fn parse_timeout<S: AsRef<str>>(timeout: S) -> Result<Duration> {
    Ok(Duration::from_secs(timeout.as_ref().parse::<u64>()?))
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

fn status_message(response: &Response, verbose: bool) -> Option<String> {
    match &response.status {
        Status::Ok(code) if verbose => Some(format!("✅ {} [{}]", response.uri, code)),
        Status::Redirected if verbose => Some(format!("🔀️ {}", response.uri)),
        Status::Excluded if verbose => Some(format!("👻 {}", response.uri)),
        Status::Failed(code) => Some(format!("🚫 {} [{}]", response.uri, code)),
        Status::Error(e) => Some(format!("⚡ {} ({})", response.uri, e)),
        Status::Timeout => Some(format!("⌛ {}", response.uri)),
        _ => None,
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

#[cfg(test)]
mod cli_test {
    use crate::test_utils::get_mock_server;
    use anyhow::Result;
    use assert_cmd::Command;
    use predicates::str::contains;
    use std::fs::File;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn main_command() -> Command {
        // this gets the "main" binary name (e.g. `lychee`)
        Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("Couldn't get cargo package name")
    }

    fn fixtures_path() -> PathBuf {
        Path::new(module_path!()).parent().unwrap().join("fixtures")
    }

    #[test]
    fn test_exclude_all_private() {
        let mut cmd = main_command();

        let test_all_private_path = fixtures_path().join("TEST_ALL_PRIVATE.md");

        // assert that the command runs OK, and that it excluded all the links
        cmd.arg("--exclude-all-private")
            .arg("--verbose")
            .arg(test_all_private_path)
            .assert()
            .success()
            .stdout(contains("Total: 7"))
            .stdout(contains("Excluded: 7"))
            .stdout(contains("Successful: 0"))
            .stdout(contains("Errors: 0"));
    }

    /// Test that a GitHub link can be checked without specifying the token.
    #[test]
    fn test_check_github_no_token() {
        let mut cmd = main_command();
        let test_github_path = fixtures_path().join("TEST_GITHUB.md");

        cmd.arg("--verbose")
            .arg(test_github_path)
            .assert()
            .success()
            .stdout(contains("Total: 1"))
            .stdout(contains("Excluded: 0"))
            .stdout(contains("Successful: 1"))
            .stdout(contains("Errors: 0"));
    }

    #[tokio::test]
    async fn test_failure_404_link() {
        let mut cmd = main_command();
        let mock_server = get_mock_server(http::StatusCode::NOT_FOUND).await;
        let dir = tempfile::tempdir().expect("Failed to create tempdir");
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).expect("Failed to create tempfile");

        writeln!(file, "{}", mock_server.uri()).expect("Failed to write to file");

        cmd.arg(file_path)
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[test]
    fn test_failure_github_404_no_token() {
        let mut cmd = main_command();
        let test_github_404_path = fixtures_path().join("TEST_GITHUB_404.md");

        cmd.arg(test_github_404_path)
            .env_clear()
            .assert()
            .failure()
            .code(2)
            .stdout(contains("https://github.com/mre/idiomatic-rust-doesnt-exist-man \
            (GitHub token not specified. To check GitHub links reliably, use `--github-token` flag / `GITHUB_TOKEN` env var.)"));
    }

    #[tokio::test]
    async fn test_stdin_input() {
        let mut cmd = main_command();
        let mock_server = get_mock_server(http::StatusCode::OK).await;

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .success();
    }

    #[tokio::test]
    async fn test_stdin_input_failure() {
        let mut cmd = main_command();
        let mock_server = get_mock_server(http::StatusCode::INTERNAL_SERVER_ERROR).await;

        cmd.arg("-")
            .write_stdin(mock_server.uri())
            .assert()
            .failure()
            .code(2);
    }

    #[tokio::test]
    async fn test_stdin_input_multiple() {
        let mut cmd = main_command();
        let mock_server_a = get_mock_server(http::StatusCode::OK).await;
        let mock_server_b = get_mock_server(http::StatusCode::OK).await;

        // this behavior (treating multiple `-` as separate inputs) is the same as most CLI tools
        // that accept `-` as stdin, e.g. `cat`, `bat`, `grep` etc.
        cmd.arg("-")
            .arg("-")
            .write_stdin(mock_server_a.uri())
            .write_stdin(mock_server_b.uri())
            .assert()
            .success();
    }

    #[test]
    fn test_missing_file_error() {
        let mut cmd = main_command();
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4().to_string());

        cmd.arg(&filename)
            .assert()
            .failure()
            .code(1)
            .stderr(contains(format!(
                "Error: Failed to read file: `{}`",
                filename
            )));
    }

    #[test]
    fn test_missing_file_ok_if_skip_missing() {
        let mut cmd = main_command();
        let filename = format!("non-existing-file-{}", uuid::Uuid::new_v4().to_string());

        cmd.arg(&filename).arg("--skip-missing").assert().success();
    }

    #[tokio::test]
    async fn test_glob() -> Result<()> {
        // using Result to be able to use `?`
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = get_mock_server(http::StatusCode::OK).await;
        let mock_server_b = get_mock_server(http::StatusCode::OK).await;
        let mut file_a = File::create(dir.path().join("a.md"))?;
        let mut file_b = File::create(dir.path().join("b.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("Total: 2"));

        Ok(())
    }

    #[cfg(target_os = "linux")] // MacOS and Windows have case-insensitive filesystems
    #[tokio::test]
    async fn test_glob_ignore_case() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let mock_server_a = get_mock_server(http::StatusCode::OK).await;
        let mock_server_b = get_mock_server(http::StatusCode::OK).await;
        let mut file_a = File::create(dir.path().join("README.md"))?;
        let mut file_b = File::create(dir.path().join("readme.md"))?;

        writeln!(file_a, "{}", mock_server_a.uri().as_str())?;
        writeln!(file_b, "{}", mock_server_b.uri().as_str())?;

        cmd.arg(dir.path().join("[r]eadme.md"))
            .arg("--verbose")
            .arg("--glob-ignore-case")
            .assert()
            .success()
            .stdout(contains("Total: 2"));

        Ok(())
    }

    #[tokio::test]
    async fn test_glob_recursive() -> Result<()> {
        let mut cmd = main_command();

        let dir = tempfile::tempdir()?;
        let subdir_level_1 = tempfile::tempdir_in(&dir)?;
        let subdir_level_2 = tempfile::tempdir_in(&subdir_level_1)?;

        let mock_server = get_mock_server(http::StatusCode::OK).await;
        let mut file = File::create(subdir_level_2.path().join("test.md"))?;

        writeln!(file, "{}", mock_server.uri().as_str())?;

        // ** should be a recursive glob
        cmd.arg(dir.path().join("**/*.md"))
            .arg("--verbose")
            .assert()
            .success()
            .stdout(contains("Total: 1"));

        Ok(())
    }
}
