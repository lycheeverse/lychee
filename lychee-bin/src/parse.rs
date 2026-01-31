use anyhow::{Context, Result};
use lychee_lib::{BaseInfo, remap::Remaps};
use std::time::Duration;

/// Parse seconds into a `Duration`
pub(crate) const fn parse_duration_secs(secs: usize) -> Duration {
    Duration::from_secs(secs as u64)
}

/// Parse URI remaps
pub(crate) fn parse_remaps(remaps: &[String]) -> Result<Remaps> {
    Remaps::try_from(remaps)
        .context("Remaps must be of the form '<pattern> <uri>' (separated by whitespace)")
}

pub(crate) fn parse_base_info(src: &str) -> Result<BaseInfo> {
    match BaseInfo::try_from(src) {
        Ok(x) => Ok(x),
        Err(e) => {
            // if context is defined, clap displays only the context string in
            // argument parse errors. to keep the message from within InvalidBase,
            // we need to retain it manually.
            let message = format!(
                "{e}. See `--help` for more information. If you want to resolve \
                root-relative links in local files, also see `--root-dir`."
            );
            Err(e).context(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;

    #[test]
    fn test_parse_remap() {
        let remaps =
            parse_remaps(&["https://example.com http://127.0.0.1:8080".to_string()]).unwrap();
        assert_eq!(remaps.len(), 1);
        let (pattern, url) = remaps[0].to_owned();
        assert_eq!(
            pattern.to_string(),
            Regex::new("https://example.com").unwrap().to_string()
        );
        assert_eq!(url, "http://127.0.0.1:8080");
    }
}
