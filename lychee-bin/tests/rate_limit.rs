use assert_cmd::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Rate limit delay (in seconds) encoded in the test headers.
const RATE_LIMIT_DELAY_SECONDS: u64 = 1;

/// Reset offset for absolute-timestamp headers. The extra second lets lychee
/// send the first request and parse the headers before the window resets.
const RATE_LIMIT_RESET_OFFSET_SECONDS: u64 = RATE_LIMIT_DELAY_SECONDS + 1;

/// Allowed deviation (either direction) between measured and expected delay.
/// Absorbs scheduling jitter, process spawn, and the HTTP round-trip.
const TIMING_TOLERANCE: Duration = Duration::from_millis(1000);

/// How the reset point is encoded in the response headers.
enum Reset {
    /// Absolute Unix timestamp (GitHub, GitLab); lychee waits `reset - now`.
    Absolute(SystemTime),
    /// Relative seconds until reset (IETF draft, `Retry-After`); lychee waits
    /// that long after parsing the headers.
    Relative(Duration),
}

/// Absolute reset time `RATE_LIMIT_RESET_OFFSET_SECONDS` from now, truncated to
/// whole seconds to match a Unix-timestamp header.
fn absolute_reset_time() -> SystemTime {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + RATE_LIMIT_RESET_OFFSET_SECONDS;
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// Formats a `SystemTime` as a whole-second Unix timestamp string.
fn unix_timestamp(time: SystemTime) -> String {
    time.duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

/// Runs lychee sequentially over two paths and asserts the gap between the two
/// server-side requests is within `TIMING_TOLERANCE` of the expected delay.
///
/// For [`Reset::Absolute`] the expected delay is derived from the first
/// request's arrival; for [`Reset::Relative`] it is the encoded delay.
async fn run_rate_limit_test(headers: &[(&str, &str)], reset: Reset) {
    let mock_server = MockServer::start().await;
    let request_times = Arc::new(Mutex::new(Vec::<(Instant, SystemTime)>::new()));

    // Two paths are needed because the backoff applies *between* requests: the
    // first receives the rate-limit headers, the second is delayed.
    let paths = ["path1", "path2"];

    for path_str in paths {
        let mut template = ResponseTemplate::new(200);
        for (key, value) in headers {
            template = template.insert_header(*key, *value);
        }

        let request_times = request_times.clone();
        Mock::given(method("GET"))
            .and(path(format!("/{path_str}")))
            .respond_with(move |_: &wiremock::Request| {
                request_times
                    .lock()
                    .unwrap()
                    .push((Instant::now(), SystemTime::now()));
                template.clone()
            })
            .expect(1)
            .mount(&mock_server)
            .await;
    }

    let inputs: String = paths
        .iter()
        .map(|p| format!("{uri}/{p}\n", uri = mock_server.uri()))
        .collect();

    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        .arg("--max-concurrency")
        .arg("1") // sequential so backoffs are measurable
        .write_stdin(inputs)
        .assert()
        .success();

    let (elapsed, first_request_at) = {
        let times = request_times.lock().unwrap();
        assert_eq!(
            times.len(),
            paths.len(),
            "Expected {} requests but recorded {}; backoff timing is meaningless",
            paths.len(),
            times.len()
        );

        let (first_instant, first_system) = *times.first().unwrap();
        let (last_instant, _) = *times.last().unwrap();
        (last_instant.duration_since(first_instant), first_system)
    };

    // Absolute timestamps derive the expected delay from the first request's
    // actual arrival, avoiding flakiness from second-rounding and spawn time.
    let expected_delay = match reset {
        Reset::Relative(delay) => delay,
        Reset::Absolute(reset_at) => reset_at
            .duration_since(first_request_at)
            .unwrap_or(Duration::ZERO),
    };

    let expected_min = expected_delay.saturating_sub(TIMING_TOLERANCE);
    let expected_max = expected_delay.saturating_add(TIMING_TOLERANCE);

    assert!(
        elapsed >= expected_min,
        "Rate limit headers were not respected! Expected at least {expected_min:?}, but got {elapsed:?}"
    );
    assert!(
        elapsed <= expected_max,
        "Rate limit wait was unexpectedly long! Expected at most {expected_max:?}, but got {elapsed:?}"
    );

    mock_server.verify().await;
}

#[tokio::test]
async fn test_github_rate_limit_exhausted() {
    let reset_time = absolute_reset_time();
    let reset_header = unix_timestamp(reset_time);

    run_rate_limit_test(
        &[
            ("x-ratelimit-limit", "100"),
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", &reset_header),
        ],
        Reset::Absolute(reset_time),
    )
    .await;
}

#[tokio::test]
async fn test_gitlab_rate_limit_exhausted() {
    let reset_time = absolute_reset_time();
    let reset_header = unix_timestamp(reset_time);

    run_rate_limit_test(
        &[
            ("RateLimit-Limit", "100"),
            ("RateLimit-Remaining", "0"),
            ("RateLimit-Reset", &reset_header),
            // GitLab-specific. Its presence tells the `rate-limits` crate to read
            // `RateLimit-Reset` as an absolute timestamp rather than relative
            // seconds (IETF draft), which share the same header name.
            ("RateLimit-Observed", "100"),
        ],
        Reset::Absolute(reset_time),
    )
    .await;
}

#[tokio::test]
async fn test_ietf_draft_exhausted() {
    let delay_str = RATE_LIMIT_DELAY_SECONDS.to_string();

    run_rate_limit_test(
        &[
            ("RateLimit-Limit", "100"),
            ("RateLimit-Remaining", "0"),
            // Relative seconds (IETF draft), unlike GitLab's absolute timestamp.
            ("RateLimit-Reset", &delay_str),
        ],
        Reset::Relative(Duration::from_secs(RATE_LIMIT_DELAY_SECONDS)),
    )
    .await;
}

#[tokio::test]
async fn test_retry_rate_limit_headers() {
    const RETRY_DELAY: Duration = Duration::from_secs(1);
    let server = MockServer::start().await;

    // Record timestamps in the handlers but assert in the test body: a panic
    // inside a handler surfaces as a confusing connection error.
    let first_request_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let second_request_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let first_time_clone = first_request_time.clone();
    Mock::given(method("GET"))
        .respond_with(move |_: &wiremock::Request| {
            *first_time_clone.lock().unwrap() = Some(Instant::now());
            ResponseTemplate::new(429)
                .insert_header("Retry-After", RETRY_DELAY.as_secs().to_string().as_str())
        })
        .expect(1)
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let second_time_clone = second_request_time.clone();
    Mock::given(method("GET"))
        .respond_with(move |_: &wiremock::Request| {
            *second_time_clone.lock().unwrap() = Some(Instant::now());
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&server)
        .await;

    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        // Isolate the `Retry-After`-driven delay from the generic retry wait.
        .arg("--retry-wait-time")
        .arg("0")
        .write_stdin(server.uri())
        .assert()
        .success();

    server.verify().await;

    let first = first_request_time
        .lock()
        .unwrap()
        .expect("first request was never received");
    let second = second_request_time
        .lock()
        .unwrap()
        .expect("retry was never received");
    let elapsed = second.duration_since(first);

    let expected_min = RETRY_DELAY.saturating_sub(TIMING_TOLERANCE);
    let expected_max = RETRY_DELAY.saturating_add(TIMING_TOLERANCE);

    assert!(
        elapsed >= expected_min,
        "Retry-After was not respected: expected at least {expected_min:?}, got {elapsed:?}"
    );
    assert!(
        elapsed <= expected_max,
        "Retry wait was unexpectedly long: expected at most {expected_max:?}, got {elapsed:?}"
    );
}

#[tokio::test]
async fn test_retry_after_seconds() {
    // A `Retry-After` on a 429 should delay the *next* request to a different
    // path on the same host. Unlike `test_retry_rate_limit_headers` (same URL),
    // this checks that the backoff carries over to an independent URL.
    let mock_server = MockServer::start().await;
    let request_times = Arc::new(Mutex::new(Vec::<Instant>::new()));
    let delay_str = RATE_LIMIT_DELAY_SECONDS.to_string();

    // path1: 429 + Retry-After on the first hit, 200 on the retry.
    let times_clone_1 = request_times.clone();
    let delay_header = delay_str.clone();
    Mock::given(method("GET"))
        .and(path("/path1"))
        .respond_with(move |_: &wiremock::Request| {
            times_clone_1.lock().unwrap().push(Instant::now());
            ResponseTemplate::new(429).insert_header("Retry-After", delay_header.as_str())
        })
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    // Serves the path1 retry once the backoff expires.
    Mock::given(method("GET"))
        .and(path("/path1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let times_clone_2 = request_times.clone();
    Mock::given(method("GET"))
        .and(path("/path2"))
        .respond_with(move |_: &wiremock::Request| {
            times_clone_2.lock().unwrap().push(Instant::now());
            ResponseTemplate::new(200)
        })
        .expect(1)
        .mount(&mock_server)
        .await;

    let inputs: String = ["path1", "path2"]
        .iter()
        .map(|p| format!("{}/{p}\n", mock_server.uri()))
        .collect();

    let mut cmd = Command::cargo_bin("lychee").unwrap();
    cmd.arg("-")
        .arg("--max-concurrency")
        .arg("1") // sequential so the backoff order is deterministic
        // Isolate the Retry-After-driven delay from the generic retry wait.
        .arg("--retry-wait-time")
        .arg("0")
        .write_stdin(inputs)
        .assert()
        .success();

    let elapsed = {
        let times = request_times.lock().unwrap();
        assert_eq!(
            times.len(),
            2,
            "Expected 2 recorded requests (path1 first hit + path2), got {}",
            times.len()
        );
        times[1].duration_since(times[0])
    };

    let expected_delay = Duration::from_secs(RATE_LIMIT_DELAY_SECONDS);
    let expected_min = expected_delay.saturating_sub(TIMING_TOLERANCE);
    let expected_max = expected_delay.saturating_add(TIMING_TOLERANCE);

    assert!(
        elapsed >= expected_min,
        "Retry-After backoff not respected: expected at least {expected_min:?}, got {elapsed:?}"
    );
    assert!(
        elapsed <= expected_max,
        "Retry-After backoff was too long: expected at most {expected_max:?}, got {elapsed:?}"
    );

    mock_server.verify().await;
}
