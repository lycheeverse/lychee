use crate::options::Config;
use crate::parse::{parse_basic_auth, parse_duration_secs, parse_headers, parse_remaps};
use anyhow::{Context, Result};
use headers::HeaderMapExt;
use http::StatusCode;
use lychee_lib::{Client, ClientBuilder};
use regex::RegexSet;
use std::{collections::HashSet, str::FromStr};

/// Creates a client according to the command-line config
pub(crate) fn create(cfg: &Config) -> Result<Client> {
    let mut headers = parse_headers(&cfg.header)?;
    if let Some(auth) = &cfg.basic_auth {
        let auth_header = parse_basic_auth(auth)?;
        headers.typed_insert(auth_header);
    }

    let timeout = parse_duration_secs(cfg.timeout);
    let retry_wait_time = parse_duration_secs(cfg.retry_wait_time);
    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;

    let remaps = parse_remaps(&cfg.remap)?;
    let includes = RegexSet::new(&cfg.include)?;
    let excludes = RegexSet::new(&cfg.exclude)?;

    // Offline mode overrides the scheme
    let schemes = if cfg.offline {
        vec!["file".to_string()]
    } else {
        cfg.scheme.clone()
    };

    let accepted = match cfg.accept {
        Some(ref accepted) => {
            let accepted: Result<HashSet<_>, _> = accepted
                .iter()
                .map(|code| StatusCode::from_u16(*code))
                .collect();
            Some(accepted?)
        }
        None => None,
    };

    ClientBuilder::builder()
        .remaps(remaps)
        .includes(includes)
        .excludes(excludes)
        .exclude_all_private(cfg.exclude_all_private)
        .exclude_private_ips(cfg.exclude_private)
        .exclude_link_local_ips(cfg.exclude_link_local)
        .exclude_loopback_ips(cfg.exclude_loopback)
        .exclude_mail(cfg.exclude_mail)
        .max_redirects(cfg.max_redirects)
        .user_agent(cfg.user_agent.clone())
        .allow_insecure(cfg.insecure)
        .custom_headers(headers)
        .method(method)
        .timeout(timeout)
        .retry_wait_time(retry_wait_time)
        .github_token(cfg.github_token.clone())
        .schemes(HashSet::from_iter(schemes))
        .accepted(accepted)
        .require_https(cfg.require_https)
        .build()
        .client()
        .context("Failed to create request client")
}
