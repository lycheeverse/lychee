#[macro_use]
extern crate log;

use anyhow::anyhow;
use anyhow::Result;
use futures::future::join_all;
use gumdrop::Options;
use regex::RegexSet;
use reqwest::{
    header::{HeaderMap, HeaderName},
    Url,
};
use std::{collections::HashSet, convert::TryInto, env, time::Duration};

mod checker;
mod collector;
mod extract;
mod options;

use checker::{Checker, Status};
use options::LycheeOptions;

fn print_summary(found: &HashSet<Url>, results: &Vec<Status>) {
    let found = found.len();
    let excluded: usize = results
        .iter()
        .filter(|l| matches!(l, Status::Excluded))
        .count();
    let success: usize = results
        .iter()
        .filter(|l| matches!(l, Status::Ok(_)))
        .count();
    let errors: usize = found - excluded - success;

    println!("");
    println!("📝Summary");
    println!("-------------------");
    println!("🔍Found: {}", found);
    println!("👻Excluded: {}", excluded);
    println!("✅Successful: {}", success);
    println!("🚫Errors: {}", errors);
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let opts = LycheeOptions::parse_args_default_or_exit();

    let mut runtime = match opts.threads {
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
    let errorcode = runtime.block_on(run(opts))?;
    std::process::exit(errorcode);
}

async fn run(opts: LycheeOptions) -> Result<i32> {
    let excludes = RegexSet::new(opts.exclude).unwrap();
    let headers = parse_headers(opts.headers)?;
    let accepted = match opts.accept {
        Some(accept) => parse_statuscodes(accept)?,
        None => None,
    };
    let timeout = parse_timeout(opts.timeout)?;

    let checker = Checker::try_new(
        env::var("GITHUB_TOKEN")?,
        Some(excludes),
        opts.max_redirects,
        opts.user_agent,
        opts.insecure,
        opts.scheme,
        headers,
        opts.method.try_into()?,
        accepted,
        Some(timeout),
        opts.verbose,
    )?;

    let links = collector::collect_links(opts.inputs).await?;
    let futures: Vec<_> = links.iter().map(|l| checker.check(&l)).collect();
    let results = join_all(futures).await;

    if opts.verbose {
        print_summary(&links, &results);
    }
    Ok(results.iter().all(|r| r.is_success()) as i32)
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

fn parse_statuscodes(accept: String) -> Result<Option<HashSet<http::StatusCode>>> {
    let mut statuscodes = HashSet::new();
    for code in accept.split(',').into_iter() {
        let code: reqwest::StatusCode = reqwest::StatusCode::from_bytes(code.as_bytes())?;
        statuscodes.insert(code);
    }
    Ok(Some(statuscodes))
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
        let expected: Option<HashSet<StatusCode>> = Some(
            [
                StatusCode::OK,
                StatusCode::NO_CONTENT,
                StatusCode::MOVED_PERMANENTLY,
            ]
            .iter()
            .cloned()
            .collect(),
        );
        assert_eq!(actual, expected);
    }
}
