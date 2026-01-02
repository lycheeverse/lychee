use anyhow::{Context, Result};

use super::StatsFormatter;
use crate::formatters::stats::OutputStats;

pub(crate) struct Json;

impl Json {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Json {
    /// Format stats as JSON object
    fn format(&self, stats: OutputStats) -> Result<String> {
        serde_json::to_string_pretty(&stats).context("Cannot format stats as JSON")
    }
}

#[cfg(test)]
mod tests {
    use crate::formatters::stats::{Json, StatsFormatter, get_dummy_stats};

    #[test]
    fn test_json_formatter() {
        let formatter = Json::new();
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(
            result,
            r#"{
  "total": 2,
  "successful": 0,
  "unknown": 0,
  "unsupported": 0,
  "timeouts": 0,
  "redirects": 1,
  "excludes": 0,
  "errors": 1,
  "cached": 0,
  "success_map": {},
  "error_map": {
    "https://example.com/": [
      {
        "url": "https://github.com/mre/idiomatic-rust-doesnt-exist-man",
        "status": {
          "text": "404 Not Found",
          "code": 404
        }
      }
    ]
  },
  "suggestion_map": {
    "https://example.com/": [
      {
        "original": "https://original.dev/",
        "suggestion": "https://suggestion.dev/"
      }
    ]
  },
  "redirect_map": {
    "https://example.com/": [
      {
        "url": "https://redirected.dev/",
        "status": {
          "text": "Redirect",
          "code": 200,
          "redirects": [
            "https://1.dev/",
            "https://2.dev/",
            "http://redirected.dev/"
          ]
        }
      }
    ]
  },
  "excluded_map": {},
  "duration_secs": 0,
  "detailed_stats": true,
  "host_stats": {
    "example.com": {
      "total_requests": 5,
      "successful_requests": 3,
      "success_rate": 0.6,
      "rate_limited": 1,
      "client_errors": 0,
      "server_errors": 1,
      "median_request_time_ms": null,
      "cache_hits": 1,
      "cache_misses": 4,
      "cache_hit_rate": 0.2,
      "status_codes": {}
    }
  }
}"#
        );
    }
}
