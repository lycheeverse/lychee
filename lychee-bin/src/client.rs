use crate::options::Config;
use crate::parse::{parse_duration_secs, parse_headers, parse_remaps};
use anyhow::{Context, Result};
use http::StatusCode;
use lychee_lib::{Client, ClientBuilder};
use regex::RegexSet;
use reqwest_cookie_store::CookieStoreMutex;
use std::sync::Arc;
use std::{collections::HashSet, str::FromStr};

/// Creates a client according to the command-line config
pub(crate) fn create(cfg: &Config, cookie_jar: Option<&Arc<CookieStoreMutex>>) -> Result<Client> {
    let headers = parse_headers(&cfg.header)?;
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

    let accepted = cfg
        .accept
        .clone()
        .into_set()
        .iter()
        .map(|value| StatusCode::from_u16(*value))
        .collect::<Result<HashSet<_>, _>>()?;

    // `exclude_mail` will be removed in 1.0. Until then, we need to support it.
    // Therefore, we need to check if both `include_mail` and `exclude_mail` are set to `true`
    // and return an error if that's the case.
    if cfg.include_mail && cfg.exclude_mail {
        return Err(anyhow::anyhow!(
            "Cannot set both `include-mail` and `exclude-mail` to true"
        ));
    }

    // By default, clap sets `exclude_mail` to `false`.
    // Therefore, we need to check if `exclude_mail` is explicitly set to
    // `true`. If so, we need to set `include_mail` to `false`.
    // Otherwise, we use the value of `include_mail`.
    let include_mail = if cfg.exclude_mail {
        false
    } else {
        cfg.include_mail
    };

    ClientBuilder::builder()
        .remaps(remaps)
        .base(cfg.base.clone())
        .includes(includes)
        .excludes(excludes)
        .exclude_all_private(cfg.exclude_all_private)
        .exclude_private_ips(cfg.exclude_private)
        .exclude_link_local_ips(cfg.exclude_link_local)
        .exclude_loopback_ips(cfg.exclude_loopback)
        .include_mail(include_mail)
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
        .include_fragments(cfg.include_fragments)
        .fallback_extensions(cfg.fallback_extensions.clone())
        .build()
        .client()
        .context("Failed to create request client")
}
