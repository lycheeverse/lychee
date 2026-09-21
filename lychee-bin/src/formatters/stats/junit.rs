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
    /// Format stats as a `JUnit` XML report
    fn format(&self, stats: OutputStats) -> Result<String> {
        junit_xml(stats.response_stats)
            .to_string()
            .context("Unable to convert JUnit report to XML")
    }
}

/// Unfortunately there is no official specification of this format,
/// but there is documentation available at <https://github.com/testmoapp/junitxml>.
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
    );
    let skipped = junit_testcases_group(stats.excluded_map, TestCaseStatus::skipped());
    let successes = junit_testcases_group(stats.success_map, TestCaseStatus::success());

    [failures, skipped, successes].concat()
}

fn junit_testcases_group(
    map: HashMap<InputSource, HashSet<ResponseBody>>,
    status: TestCaseStatus,
) -> Vec<TestCase> {
    map.into_iter()
        .flat_map(move |(source, b)| {
            let status = status.clone();
            b.into_iter().map(move |response| {
                // Identify the link occurrence, not its outcome, so CI can track
                // the same test when a broken link is fixed.
                let name = match response.span {
                    Some(span) => format!("{source}:{span} - {}", response.uri),
                    None => format!("{source} - {}", response.uri),
                };
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
    use lychee_lib::{ErrorKind, InputSource, RawUriSpan, ResponseBody, Status};
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use url::Url;

    use crate::formatters::stats::{
        self, OutputStats, ResponseStats, StatsFormatter, junit::Junit,
    };

    use super::junit_testcases;

    #[test]
    fn test_junit_formatter() {
        let formatter = Junit::new();
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(
            result,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="lychee link check results" tests="3" skipped="1" failures="1" errors="0">
    <testsuite name="lychee link check results" tests="3" skipped="1" errors="0" failures="1">
        <testcase name="https://example.com/:1:1 - https://github.com/mre/idiomatic-rust-doesnt-exist-man" time="1.000" file="https://example.com/" line="1">
            <failure message="https://github.com/mre/idiomatic-rust-doesnt-exist-man (at 1:1) | Rejected status code: 404 Not Found"/>
            <system-out>https://github.com/mre/idiomatic-rust-doesnt-exist-man (at 1:1) | Rejected status code: 404 Not Found</system-out>
        </testcase>
        <testcase name="https://example.com/ - https://excluded.org/" time="0.042" file="https://example.com/">
            <skipped message="https://excluded.org/ | This is due to your &apos;exclude&apos; values"/>
            <system-out>https://excluded.org/ | This is due to your &apos;exclude&apos; values</system-out>
        </testcase>
        <testcase name="https://example.com/ - https://success.org/" time="1.000" file="https://example.com/">
            <system-out>https://success.org/</system-out>
        </testcase>
    </testsuite>
</testsuites>
"#
        );
    }

    fn response(span: Option<RawUriSpan>, status: Status) -> ResponseBody {
        ResponseBody {
            uri: "https://example.com/link".try_into().unwrap(),
            status,
            redirects: None,
            remap: None,
            span,
            duration: None,
        }
    }

    fn span(line: usize, column: Option<usize>) -> RawUriSpan {
        RawUriSpan {
            line: line.try_into().unwrap(),
            column: column.map(|value| value.try_into().unwrap()),
        }
    }

    #[rstest]
    #[case::without_position(None, "")]
    #[case::with_position(Some(span(1, Some(1))), ":1:1")]
    fn same_url_in_different_files_has_distinct_names(
        #[case] position: Option<RawUriSpan>,
        #[case] location: &str,
    ) {
        let stats = ResponseStats {
            error_map: ["README.md", "guide.md"]
                .into_iter()
                .map(|path| {
                    (
                        InputSource::FsPath(path.into()),
                        HashSet::from([response(
                            position,
                            Status::Error(ErrorKind::RejectedStatusCode(StatusCode::NOT_FOUND)),
                        )]),
                    )
                })
                .collect(),
            ..Default::default()
        };
        let cases = junit_testcases(stats);
        assert_eq!(cases.len(), 2);
        let names: HashSet<_> = cases.iter().map(|case| case.name.to_string()).collect();

        assert_eq!(
            names,
            HashSet::from([
                format!("README.md{location} - https://example.com/link"),
                format!("guide.md{location} - https://example.com/link"),
            ])
        );
    }

    #[test]
    fn same_url_at_different_positions_has_distinct_names() {
        let stats = ResponseStats {
            success_map: HashMap::from([(
                InputSource::FsPath("README.md".into()),
                [
                    span(1, Some(1)),
                    span(1, Some(20)),
                    span(2, Some(1)),
                    span(3, None),
                ]
                .into_iter()
                .map(|span| response(Some(span), Status::Ok(StatusCode::OK)))
                .collect(),
            )]),
            ..Default::default()
        };
        let cases = junit_testcases(stats);
        assert_eq!(cases.len(), 4);
        let names: HashSet<_> = cases.iter().map(|case| case.name.to_string()).collect();
        assert_eq!(
            names,
            [
                "README.md:1:1 - https://example.com/link",
                "README.md:1:20 - https://example.com/link",
                "README.md:2:1 - https://example.com/link",
                "README.md:3 - https://example.com/link",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
    }

    #[rstest]
    #[case::success(Status::Ok(StatusCode::OK))]
    #[case::failure(Status::Error(ErrorKind::RejectedStatusCode(StatusCode::NOT_FOUND)))]
    #[case::excluded(Status::Excluded)]
    fn testcase_name_does_not_depend_on_status(#[case] status: Status) {
        let mut stats = ResponseStats::default();
        let map = if status.is_success() {
            &mut stats.success_map
        } else if status.is_error() {
            &mut stats.error_map
        } else {
            &mut stats.excluded_map
        };
        map.insert(
            InputSource::FsPath("README.md".into()),
            HashSet::from([response(Some(span(12, Some(3))), status)]),
        );
        let cases = junit_testcases(stats);
        assert_eq!(cases.len(), 1);
        assert_eq!(
            cases[0].name.as_str(),
            "README.md:12:3 - https://example.com/link"
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
                redirects: None,
                remap: None,
                span: None,
                duration: Some(Duration::from_secs(1)),
            }]),
        )]);

        stats.response_stats.excluded_map = HashMap::from([(
            source.clone(),
            HashSet::from([ResponseBody {
                uri: "https://excluded.org".try_into().unwrap(),
                status: Status::Excluded,
                redirects: None,
                remap: None,
                span: None,
                duration: Some(Duration::from_millis(42)),
            }]),
        )]);

        stats
    }
}
