use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use lychee_lib::{InputSource, ResponseBody};
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};

use super::StatsFormatter;
use crate::formatters::stats::{OutputStats, ResponseStats};

/// The `JUnit` XML report format.
/// This format can be imported on code forges (e.g. GitHub & GitLab)
/// to create useful annotations where failing links are detected.
pub(crate) struct Junit {}

impl Junit {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Junit {
    /// Format stats as JSON object
    fn format(&self, stats: OutputStats) -> Result<String> {
        junit_xml(stats.response_stats)
            .to_string()
            .context("Unable to convert JUnit report to XML")
    }
}

/// Unfortunately there is no official specification of this format,
/// but there is documentation available at <https://github.com/testmoapp/junitxml>.
/// Note that using a library would be overkill in this case.
fn junit_xml(stats: ResponseStats) -> Report {
    const NAME: &str = "lychee link check results";
    let mut report = Report::new(NAME);

    let mut test_suite = TestSuite::new(NAME);
    test_suite.add_test_cases(junit_testcases(stats));

    report.add_test_suite(test_suite);
    report
}

fn junit_testcases(stats: ResponseStats) -> Vec<TestCase> {
    let failures = junit_testcases_group(
        stats.error_map,
        TestCaseStatus::non_success(NonSuccessKind::Failure),
        "Failed",
    );
    let skipped = junit_testcases_group(stats.excluded_map, TestCaseStatus::skipped(), "Excluded");
    let successes =
        junit_testcases_group(stats.success_map, TestCaseStatus::success(), "Successful");
    let redirected =
        junit_testcases_group(stats.redirect_map, TestCaseStatus::success(), "Redirected");

    [failures, skipped, successes, redirected].concat()
}

fn junit_testcases_group(
    map: HashMap<InputSource, HashSet<ResponseBody>>,
    status: TestCaseStatus,
    reason: &str,
) -> Vec<TestCase> {
    map.into_iter()
        .flat_map(move |(source, b)| {
            let status = status.clone();
            b.into_iter().map(move |response| {
                let name = format!("{reason} {}", response.uri);
                let mut testcase = TestCase::new(name, status.clone());
                testcase.time = response.duration;

                testcase
                    .extra
                    .insert("file".into(), source.to_string().into());

                if let Some(span) = response.span {
                    testcase
                        .extra
                        .insert("line".into(), span.line.to_string().into());
                }

                testcase.set_system_out(response.to_string());
                testcase.status.set_message(response.to_string());

                testcase
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        time::Duration,
    };

    use http::StatusCode;
    use lychee_lib::{InputSource, ResponseBody, Status};
    use pretty_assertions::assert_eq;
    use url::Url;

    use crate::formatters::stats::{self, OutputStats, StatsFormatter, junit::Junit};

    #[test]
    fn test_junit_formatter() {
        let formatter = Junit::new();
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(
            result,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="lychee link check results" tests="4" failures="1" errors="0">
    <testsuite name="lychee link check results" tests="4" disabled="1" errors="0" failures="1">
        <testcase name="Failed https://github.com/mre/idiomatic-rust-doesnt-exist-man" time="1.000" file="https://example.com/" line="1">
            <failure message="https://github.com/mre/idiomatic-rust-doesnt-exist-man (at 1:1) | 404 Not Found: Not Found"/>
            <system-out>https://github.com/mre/idiomatic-rust-doesnt-exist-man (at 1:1) | 404 Not Found: Not Found</system-out>
        </testcase>
        <testcase name="Excluded https://excluded.org/" time="0.042" file="https://example.com/">
            <skipped message="https://excluded.org/"/>
            <system-out>https://excluded.org/</system-out>
        </testcase>
        <testcase name="Successful https://success.org/" time="1.000" file="https://example.com/">
            <system-out>https://success.org/</system-out>
        </testcase>
        <testcase name="Redirected https://redirected.dev/" time="1.000" file="https://example.com/" line="1">
            <system-out>https://redirected.dev/ (at 1:1) | Redirect: Followed 2 redirects resolving to the final status of: OK. Redirects: https://1.dev/ --[308]--&gt; https://2.dev/ --[308]--&gt; http://redirected.dev/</system-out>
        </testcase>
    </testsuite>
</testsuites>
"#
        );
    }

    fn get_dummy_stats() -> OutputStats {
        let mut stats = stats::get_dummy_stats();
        stats.response_stats.total += 2;
        stats.response_stats.successful += 1;
        stats.response_stats.excludes += 1;

        let source = InputSource::RemoteUrl(Box::new(Url::parse("https://example.com").unwrap()));

        stats.response_stats.success_map = HashMap::from([(
            source.clone(),
            HashSet::from([ResponseBody {
                uri: "https://success.org".try_into().unwrap(),
                status: Status::Ok(StatusCode::OK),
                span: None,
                duration: Some(Duration::from_secs(1)),
            }]),
        )]);

        stats.response_stats.excluded_map = HashMap::from([(
            source.clone(),
            HashSet::from([ResponseBody {
                uri: "https://excluded.org".try_into().unwrap(),
                status: Status::Excluded,
                span: None,
                duration: Some(Duration::from_millis(42)),
            }]),
        )]);

        stats
    }
}
