use std::collections::{HashMap, HashSet};

use anyhow::Result;
use lychee_lib::{InputSource, ResponseBody};

use super::StatsFormatter;
use crate::formatters::stats::{OutputStats, ResponseStats};

/// The JUnit XML report format.
/// This format can be imported on code forges (e.g. GitHub & GitLab)
/// to create useful annotations where failing links are detected.
pub(crate) struct Junit {}

impl Junit {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl StatsFormatter for Junit {
    /// Format stats as JSON object
    fn format(&self, stats: OutputStats) -> Result<String> {
        Ok(junit_xml(stats.response_stats))
    }
}

/// Unfortunately there is no official specification of this format,
/// but there is documentation available at <https://github.com/testmoapp/junitxml>.
/// Note that using a library would be overkill in this case.
fn junit_xml(stats: ResponseStats) -> String {
    let testcases = junit_testcases(stats);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites><testsuite name="lychee link check results">{testcases}</testsuite></testsuites>"#
    )
}

fn junit_testcases(stats: ResponseStats) -> String {
    let failures = junit_testcases_group(stats.error_map, Some("failure"), "Failed");
    let skipped = junit_testcases_group(stats.excluded_map, Some("skipped"), "Excluded");
    let sucesses = junit_testcases_group(stats.success_map, None, "Successful");
    let redirected = junit_testcases_group(stats.redirect_map, None, "Redirected");

    format!("{failures}\n\n{skipped}\n\n{sucesses}\n\n{redirected}\n")
}

fn junit_testcases_group(
    map: HashMap<InputSource, HashSet<ResponseBody>>,
    tag: Option<&str>,
    reason: &str,
) -> String {
    if map.is_empty() {
        return "".into();
    }

    let xml = map
        .into_iter()
        .flat_map(|(source, b)| {
            b.into_iter().map(move |response| {
                let file = xml_property("file", &source);
                let name = xml_property("name", format!("{reason} {}", response.uri));
                let line = response
                    .span
                    .map(|s| xml_property("line", s.line))
                    .unwrap_or_default();

                let message = xml_property("message", &response);

                let inner = tag
                    .map(|tag| format!("\n        <{tag} {message} />",))
                    .unwrap_or_default();

                format!(
                    r#"
    <testcase {name} {file} {line}>
        <system-out>{response}</system-out>{inner}
    </testcase>"#
                )
            })
        })
        .collect::<Vec<String>>()
        .join("\n");

    let comment = format!("<!-- {reason} -->");
    format!("\n    {comment}\n{xml}")
}

/// Write an XML property `key="value"` where the contents
/// of `value` is escaped.
fn xml_property<S: ToString>(key: &str, value: S) -> String {
    let value = value.to_string().replace('"', "&quot;"); // only need to escape `"`
    format!(r#"{key}="{value}""#)
}

#[cfg(test)]
mod tests {
    use crate::formatters::stats::{StatsFormatter, get_dummy_stats, junit::Junit};

    #[test]
    fn test_junit_formatter() {
        let formatter = Junit::new();
        let result = formatter.format(get_dummy_stats()).unwrap();

        assert_eq!(
            result,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites><testsuite name="lychee link check results">
    <testcase name="Broken URI https://github.com/mre/idiomatic-rust-doesnt-exist-man" file="https://example.com/">
        <failure message="https://github.com/mre/idiomatic-rust-doesnt-exist-man | 404 Not Found: Not Found">Not Found</failure>
    </testcase>
</testsuite></testsuites>
"#
        );
    }
}
