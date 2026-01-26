use crate::options::{Config, HeaderMapExt};
use crate::parse::{parse_duration_secs, parse_remaps};
use anyhow::{Context, Result};
use http::{HeaderMap, StatusCode};
use lychee_lib::StatusCodeSelector;
use lychee_lib::{Client, ClientBuilder, ratelimit::RateLimitConfig};
use regex::RegexSet;
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;
use std::{collections::HashSet, str::FromStr};

/// Creates a client according to the command-line config
pub(crate) fn create(cfg: &Config, cookie_jar: Option<&Arc<CookieStoreMutex>>) -> Result<Client> {
    let timeout = parse_duration_secs(cfg.timeout);
    let retry_wait_time = parse_duration_secs(cfg.retry_wait_time);
    let method: reqwest::Method = reqwest::Method::from_str(&cfg.method.to_uppercase())?;

    let remaps = parse_remaps(&cfg.remap)?;
    let includes = RegexSet::new(&cfg.include)?;
    let excludes = RegexSet::new(&cfg.exclude)?;
    let accepted: HashSet<StatusCode> = cfg
        .accept
        .clone()
        .unwrap_or(StatusCodeSelector::default_accepted())
        .into();

    // Offline mode overrides the scheme
    let schemes = if cfg.offline {
        vec!["file".to_string()]
    } else {
        cfg.scheme.clone()
    };

    let headers = HeaderMap::from_header_pairs(&cfg.header)?;

    ClientBuilder::builder()
        .remaps(remaps)
        .base(cfg.base_url.clone().unwrap_or_default())
        .includes(includes)
        .excludes(excludes)
        .exclude_all_private(cfg.exclude_all_private)
        .exclude_private_ips(cfg.exclude_private)
        .exclude_link_local_ips(cfg.exclude_link_local)
        .exclude_loopback_ips(cfg.exclude_loopback)
        .include_mail(cfg.include_mail)
        .max_redirects(cfg.max_redirects)
        .user_agent(cfg.user_agent.clone())
        .allow_insecure(cfg.insecure)
        .custom_headers(headers)
        .method(method)
        .timeout(timeout)
        .retry_wait_time(retry_wait_time)
        .max_retries(cfg.max_retries)
        .github_token(cfg.github_token.clone())
        .schemes(HashSet::from_iter(schemes))
        .accepted(accepted)
        .require_https(cfg.require_https)
        .cookie_jar(cookie_jar.cloned())
        .min_tls_version(cfg.min_tls.clone().map(Into::into))
        .include_fragments(cfg.include_fragments)
        .fallback_extensions(cfg.fallback_extensions.clone())
        .index_files(cfg.index_files.clone())
        .include_wikilinks(cfg.include_wikilinks)
        .rate_limit_config(RateLimitConfig::from_options(
            cfg.host_concurrency,
            cfg.host_request_interval,
        ))
        .hosts(cfg.hosts.clone())
        .build()
        .client()
        .context("Failed to create request client")
}
