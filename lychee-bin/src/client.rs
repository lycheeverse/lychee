use crate::options::{Config, HeaderMapExt};
use crate::parse::{parse_duration_secs, parse_remaps};
use anyhow::{Context, Result};
use http::{HeaderMap, StatusCode};
use lychee_lib::{
    Client, ClientBuilder,
    ratelimit::{HostPool, RateLimitConfig},
};
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
    let accepted: HashSet<StatusCode> = cfg.accept.clone().try_into()?;

    // Offline mode overrides the scheme
    let schemes = if cfg.offline {
        vec!["file".to_string()]
    } else {
        cfg.scheme.clone()
    };

    let headers = HeaderMap::from_header_pairs(&cfg.header)?;

    // Create combined headers for HostPool (includes User-Agent + custom headers)
    let mut combined_headers = headers.clone();
    combined_headers.insert(
        http::header::USER_AGENT,
        cfg.user_agent
            .parse()
            .context("Invalid User-Agent header")?,
    );

    // Create HostPool for rate limiting - always enabled for HTTP requests
    let rate_limit_config =
        RateLimitConfig::from_options(cfg.host_concurrency, cfg.request_interval);
    let cache_max_age = if cfg.cache { 3600 } else { 0 }; // 1 hour if caching enabled, disabled otherwise

    let mut host_pool = HostPool::new(
        rate_limit_config,
        cfg.hosts.clone(),
        cfg.max_concurrency,
        cache_max_age,
        combined_headers,
        cfg.max_redirects,
        Some(timeout),
        cfg.insecure,
    );

    if let Some(cookie_jar) = cookie_jar {
        host_pool = host_pool.with_cookie_jar(cookie_jar.clone());
    }

    ClientBuilder::builder()
        .remaps(remaps)
        .base(cfg.base_url.clone())
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
        .host_pool(Some(Arc::new(host_pool)))
        .build()
        .client()
        .context("Failed to create request client")
}
