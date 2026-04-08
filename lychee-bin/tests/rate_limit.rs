use assert_cmd::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Small tolerance for scheduler flakiness in CI environments
const EPSILON: Duration = Duration::from_millis(100);

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
    let request_times = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    // Register each path to respond with the provided headers
    for path_str in paths {
        let mut template = ResponseTemplate::new(status_code);
        for (key, value) in headers {
            template = template.insert_header(*key, *value);
        }

        let request_times = request_times.clone();
        Mock::given(method("GET"))
            .and(path(*path_str))
            .respond_with(move |_: &wiremock::Request| {
                request_times.lock().unwrap().push(Instant::now());
                template.clone()
            })
            .mount(&mock_server)
            .await;
    }

    let inputs: String = paths
        .iter()
        .map(|path| format!("{uri}{path}\n", uri = mock_server.uri()))
        .collect();

    // Measure how long lychee takes to process the URLs
    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        .arg("--max-concurrency")
        .arg("1") // Ensure sequential execution so backoffs accumulate predictably
        .write_stdin(inputs)
        .assert()
        .success();

    let times = request_times.lock().unwrap();
    let elapsed = times
        .last()
        .unwrap()
        .duration_since(*times.first().unwrap());

    assert!(
        elapsed >= expected_min_duration,
        "Rate limit headers were not respected! Expected minimum delay of {expected_min_duration:?}, but got {elapsed:?}"
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
        Duration::from_secs(2) - EPSILON,
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
        Duration::from_secs(2) - EPSILON,
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
        Duration::from_secs(2) - EPSILON,
    )
    .await;
}

#[tokio::test]
async fn test_retry_rate_limit_headers() {
    const RETRY_DELAY: Duration = Duration::from_secs(1);
    const TOLERANCE: Duration = Duration::from_millis(500);
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
            assert!(delta < RETRY_DELAY + TOLERANCE);
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
    run_rate_limit_test(
        200,
        &[("Retry-After", "2")],
        &["/1", "/2"],
        Duration::from_secs(2) - EPSILON,
    )
    .await;
}
