use assert_cmd::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The rate limit delay (in seconds) encoded in test headers.
const RATE_LIMIT_DELAY_SECONDS: u64 = 1;

/// For tests using absolute Unix timestamps as the reset time, we set the
/// reset time `RATE_LIMIT_DELAY_SECONDS + 1` seconds into the future.
/// The extra second gives lychee time to make the first request and parse
/// the headers before the reset window expires, ensuring a measurable
/// backoff of at least `RATE_LIMIT_DELAY_SECONDS`.
const RATE_LIMIT_RESET_OFFSET_SECONDS: u64 = RATE_LIMIT_DELAY_SECONDS + 1;

/// How far the measured delay may deviate from the expected delay, in either
/// direction, and still be considered a passing result. This absorbs scheduling
/// jitter, process spawn time, and the HTTP round-trip, which together can add
/// up to several hundred milliseconds on slow CI runners.
const TIMING_TOLERANCE: Duration = Duration::from_millis(1000);

/// How the rate-limit reset point is encoded in the response headers.
enum Reset {
    /// An absolute Unix timestamp (GitHub, GitLab). lychee computes the wait as
    /// `reset - now`, so the effective delay shrinks as time passes between
    /// sending the headers and lychee parsing them.
    ///
    /// We carry the whole-second-truncated `SystemTime` (exactly what the header
    /// encodes) so the expected delay can be derived from the moment the first
    /// request actually reaches the server. This avoids flakiness from both
    /// Unix-second truncation and process-spawn jitter.
    Absolute(SystemTime),
    /// A relative number of seconds until the window resets (IETF draft,
    /// `Retry-After`). lychee waits exactly this long after parsing the headers.
    Relative(Duration),
}

/// An absolute reset `SystemTime`, `RATE_LIMIT_RESET_OFFSET_SECONDS` from now,
/// truncated to whole seconds to match what a Unix-timestamp header encodes.
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

/// Sets up a mock server with two paths, runs lychee sequentially over both,
/// and asserts that the time between the two server-side request arrivals falls
/// within `[expected_delay - TIMING_TOLERANCE, expected_delay + TIMING_TOLERANCE]`.
///
/// For [`Reset::Absolute`] headers the expected delay is derived from the time
/// the first request reaches the server, so it reflects the wait lychee actually
/// computes (`reset - now`). For [`Reset::Relative`] headers it is the encoded
/// delay directly.
async fn run_rate_limit_test(headers: &[(&str, &str)], reset: Reset) {
    let mock_server = MockServer::start().await;
    let request_times = Arc::new(Mutex::new(Vec::<(Instant, SystemTime)>::new()));

    // We need at least two paths because rate limiting happens *between* requests.
    // The first request receives the rate-limit headers; the second is delayed.
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

    // Derive the delay lychee was expected to apply. For absolute timestamps this
    // depends on when the first request actually arrived at the server, which
    // removes the Unix-second rounding and spawn-time jitter that would otherwise
    // make the lower bound flaky.
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
            // Absolute Unix timestamp; lychee computes `reset - now` as the wait duration.
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
            // Absolute Unix timestamp — same header name as the IETF draft, but
            // different semantics (see `test_ietf_draft_exhausted` below).
            ("RateLimit-Reset", &reset_header),
            // `RateLimit-Observed` is a GitLab-specific header. Its presence tells
            // the `rate-limits` crate to interpret `RateLimit-Reset` as an absolute
            // Unix timestamp rather than as a relative second count (IETF draft).
            // Without it the two header sets would be ambiguous.
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
            // Relative seconds until the window resets (IETF draft semantics).
            // Contrast with GitLab above, where `RateLimit-Reset` is an absolute Unix
            // timestamp, distinguished by the presence of `RateLimit-Observed`.
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

    // Record server-side timestamps in both mock handlers and assert in the test
    // body. Panicking inside a mock handler surfaces as a cryptic connection error
    // rather than a clear assertion failure.
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
        // Zero out the generic retry wait to isolate only the `Retry-After`-driven delay.
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
    // Test that a `Retry-After` header on a 429 response causes lychee to apply
    // a per-host backoff *between* requests to different paths on the same host.
    // This is distinct from `test_retry_rate_limit_headers`, which verifies the
    // retry behaviour for the *same* URL; here we check that the backoff set by
    // the first response delays the second, independent URL.
    let mock_server = MockServer::start().await;
    let request_times = Arc::new(Mutex::new(Vec::<Instant>::new()));
    let delay_str = RATE_LIMIT_DELAY_SECONDS.to_string();

    // path1: 429 + Retry-After on the first hit, 200 on the subsequent retry.
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

    // Fallback for the path1 retry that arrives after the backoff expires.
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
        // Zero out the generic retry wait to isolate the Retry-After-driven delay.
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
