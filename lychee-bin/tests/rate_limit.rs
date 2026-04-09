use assert_cmd::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// The time in seconds we expect lychee to wait when rate-limited.
const RATE_LIMIT_DELAY_SECONDS: u64 = 1;

// When dealing with absolute Unix timestamps, we set the rate limit reset time
// to `RATE_LIMIT_DELAY_SECONDS` + some error margin (e.g. 2 seconds in the future).
// By the time lychee makes the first request and calculates how long to wait,
// some of that time has already passed. To avoid flaky tests, we conservatively
// assert that lychee waits at least `RATE_LIMIT_DELAY_SECONDS`.
const RATE_LIMIT_RESET_OFFSET_SECONDS: u64 = RATE_LIMIT_DELAY_SECONDS + 1;

/// Maximum acceptable delay beyond the expected rate limit duration
const EPSILON: Duration = Duration::from_millis(500);

/// Helper to calculate the absolute Unix epoch reset time in the future.
fn calculate_reset_time() -> String {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + RATE_LIMIT_RESET_OFFSET_SECONDS)
        .to_string()
}

/// A little helper, which sets up a mock server, runs lychee, and measures the
/// execution time to make sure that the correct rate-limit backoffs were
/// applied.
async fn run_rate_limit_test(headers: &[(&str, &str)]) {
    let mock_server = MockServer::start().await;
    let request_times = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    // We need at least two paths because rate limiting happens *between* requests.
    // The first request receives the rate limit headers, and the second is delayed.
    let paths = ["path1", "path2"];

    // Register each path to respond with the provided headers
    for path_str in paths {
        let mut template = ResponseTemplate::new(200);
        for (key, value) in headers {
            template = template.insert_header(*key, *value);
        }

        let request_times = request_times.clone();
        Mock::given(method("GET"))
            .and(path(format!("/{path_str}")))
            .respond_with(move |_: &wiremock::Request| {
                request_times.lock().unwrap().push(Instant::now());
                template.clone()
            })
            .mount(&mock_server)
            .await;
    }

    let inputs: String = paths
        .iter()
        .map(|p| format!("{uri}/{p}\n", uri = mock_server.uri()))
        .collect();

    // Measure how long lychee takes to process the URLs
    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        .arg("--max-concurrency")
        .arg("1") // Run sequentially so backoffs are predictable
        .write_stdin(inputs)
        .assert()
        .success();

    let times = request_times.lock().unwrap();
    let elapsed = times
        .last()
        .unwrap()
        .duration_since(*times.first().unwrap());

    let expected_delay = Duration::from_secs(RATE_LIMIT_DELAY_SECONDS);
    let expected_min_duration = expected_delay.saturating_sub(EPSILON);

    assert!(
        elapsed >= expected_min_duration,
        "Rate limit headers were not respected! Expected minimum delay of {expected_min_duration:?}, but got {elapsed:?}"
    );
}

#[tokio::test]
async fn test_github_rate_limit_exhausted() {
    let reset_time = calculate_reset_time();

    run_rate_limit_test(&[
        ("x-ratelimit-limit", "100"),
        ("x-ratelimit-remaining", "0"),
        ("x-ratelimit-reset", &reset_time),
    ])
    .await;
}

#[tokio::test]
async fn test_gitlab_rate_limit_exhausted() {
    let reset_time = calculate_reset_time();

    run_rate_limit_test(&[
        ("RateLimit-Limit", "100"),
        ("RateLimit-Remaining", "0"),
        ("RateLimit-Reset", &reset_time),
        ("RateLimit-Observed", "100"),
    ])
    .await;
}

#[tokio::test]
async fn test_ietf_draft_exhausted() {
    let delay_str = RATE_LIMIT_DELAY_SECONDS.to_string();
    run_rate_limit_test(&[
        ("RateLimit-Limit", "100"),
        ("RateLimit-Remaining", "0"),
        ("RateLimit-Reset", &delay_str),
    ])
    .await;
}

#[tokio::test]
async fn test_retry_rate_limit_headers() {
    const RETRY_DELAY: Duration = Duration::from_secs(1);
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", RETRY_DELAY.as_secs().to_string().as_str()),
        )
        .expect(1)
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let start = Instant::now();
    Mock::given(method("GET"))
        .respond_with(move |_: &wiremock::Request| {
            let delta = Instant::now().duration_since(start);
            assert!(delta >= RETRY_DELAY);
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;

    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        // Retry wait times are added on top of host-specific backoff timeout
        .arg("--retry-wait-time")
        .arg("0")
        .write_stdin(server.uri())
        .assert()
        .success();

    // Check that the server received the request with the header
    server.verify().await;
}

#[tokio::test]
async fn test_retry_after_seconds() {
    let delay_str = RATE_LIMIT_DELAY_SECONDS.to_string();
    run_rate_limit_test(&[("Retry-After", &delay_str)]).await;
}
