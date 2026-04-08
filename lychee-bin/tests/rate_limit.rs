use assert_cmd::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A little helper, which sets up a mock server, runs lychee, and measures the
/// execution time to make sure that the correct rate-limit backoffs were
/// applied.
async fn run_rate_limit_test(
    status_code: u16,
    headers: &[(&str, &str)],
    paths: &[&str],
    expected_min_duration: Duration,
) {
    let mock_server = MockServer::start().await;

    // Register each path to respond with the provided headers
    for path_str in paths {
        let mut template = ResponseTemplate::new(status_code);
        for (key, value) in headers {
            template = template.insert_header(*key, *value);
        }

        Mock::given(method("GET"))
            .and(path(*path_str))
            .respond_with(template)
            .mount(&mock_server)
            .await;
    }

    let inputs: String = paths
        .iter()
        .map(|path| format!("{uri}{path}\n", uri = mock_server.uri()))
        .collect();

    // Measure how long lychee takes to process the URLs
    let start = Instant::now();
    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        .arg("--max-concurrency")
        .arg("1") // Ensure sequential execution so backoffs accumulate predictably
        .write_stdin(inputs)
        .assert()
        .success();

    let elapsed = start.elapsed();

    assert!(
        elapsed >= expected_min_duration,
        "Rate limiting failed! Expected at least {expected}ms, got {actual}ms",
        expected = expected_min_duration.as_millis(),
        actual = elapsed.as_millis()
    );
}

#[tokio::test]
async fn test_github_rate_limit_exhausted() {
    let reset_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3;

    run_rate_limit_test(
        200,
        &[
            ("x-ratelimit-limit", "100"),
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", &reset_time.to_string()),
        ],
        &["/1", "/2"],
        // Give 100ms leeway for scheduler flakiness in CI environments
        Duration::from_millis(1900),
    )
    .await;
}

#[tokio::test]
async fn test_gitlab_rate_limit_exhausted() {
    let reset_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3;

    let reset_time_str = reset_time.to_string();

    run_rate_limit_test(
        200,
        &[
            ("RateLimit-Limit", "100"),
            ("RateLimit-Remaining", "0"),
            ("RateLimit-Reset", &reset_time_str),
            ("RateLimit-Observed", "100"),
        ],
        &["/1", "/2"],
        Duration::from_millis(1900),
    )
    .await;
}

#[tokio::test]
async fn test_ietf_draft_exhausted() {
    run_rate_limit_test(
        200,
        &[
            ("RateLimit-Limit", "100"),
            ("RateLimit-Remaining", "0"),
            ("RateLimit-Reset", "2"),
        ],
        &["/1", "/2"],
        Duration::from_millis(1900),
    )
    .await;
}

#[tokio::test]
async fn test_retry_after_seconds() {
    run_rate_limit_test(
        200,
        &[("Retry-After", "2")],
        &["/1", "/2"],
        Duration::from_millis(1900),
    )
    .await;
}
