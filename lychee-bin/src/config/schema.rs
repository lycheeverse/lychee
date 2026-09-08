//! JSON Schema representations for config values with custom deserializers.
//!
//! These types describe the file format accepted by Serde. They are separate
//! from the runtime types because several runtime types intentionally expose a
//! different command-line representation or live in `lychee-lib`, which should
//! not need to depend on Schemars.

#![allow(dead_code)]

use schemars::JsonSchema;
use std::collections::HashMap;

/// Log levels accepted by the case-insensitive verbosity deserializer.
#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct Verbosity(
    #[schemars(regex(
        pattern = "^([Ee][Rr][Rr][Oo][Rr]|[Ww][Aa][Rr][Nn]([Ii][Nn][Gg])?|[Ii][Nn][Ff][Oo]|[Dd][Ee][Bb][Uu][Gg]|[Tt][Rr][Aa][Cc][Ee])$"
    ))]
    String,
);

/// A status-code selector accepts a single integer, a selector string, or a
/// mixed list of integers and selector strings.
#[derive(JsonSchema)]
#[schemars(untagged)]
pub(super) enum StatusCodeSelector {
    Integer(StatusCode),
    String(StatusCodeSelectorString),
    List(Vec<StatusCodeSelectorValue>),
}

#[derive(JsonSchema)]
#[schemars(untagged)]
pub(super) enum StatusCodeSelectorValue {
    Integer(StatusCode),
    String(StatusRangeString),
}

#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct StatusCode(#[schemars(range(min = 100, max = 999))] u16);

#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct StatusCodeSelectorString(
    #[schemars(regex(
        pattern = r"^\s*$|^\s*([1-9][0-9]{2}|([1-9][0-9]{2})?\.\.(=?[1-9][0-9]{2})?)(\s*,\s*([1-9][0-9]{2}|([1-9][0-9]{2})?\.\.(=?[1-9][0-9]{2})?))*\s*$"
    ))]
    String,
);

#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct StatusRangeString(
    #[schemars(regex(pattern = r"^([1-9][0-9]{2}|([1-9][0-9]{2})?\.\.(=?[1-9][0-9]{2})?)$"))]
    String,
);

/// Request methods accept either a comma-separated string or a non-empty list.
#[derive(JsonSchema)]
#[schemars(untagged)]
pub(super) enum Methods {
    String(HttpMethodsString),
    List(#[schemars(length(min = 1))] Vec<HttpMethod>),
}

#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct HttpMethodsString(
    #[schemars(regex(
        pattern = r"^\s*[!#$%&'*+.^_`|~0-9A-Za-z-]+(\s*,\s*[!#$%&'*+.^_`|~0-9A-Za-z-]+)*\s*$"
    ))]
    String,
);

#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct HttpMethod(#[schemars(regex(pattern = r"^[!#$%&'*+.^_`|~0-9A-Za-z-]+$"))] String);

/// Basic-auth selectors use `<uri-regex> <username>:<password>` syntax.
#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct BasicAuthSelector(#[schemars(regex(pattern = r"^\S+ [^:\s]+:[^:\s]+$"))] String);

/// Archives supported in configuration files.
#[derive(JsonSchema)]
pub(super) enum Archive {
    #[schemars(rename = "wayback")]
    WaybackMachine,
}

/// File-based preprocessor configuration.
#[derive(JsonSchema)]
pub(super) struct Preprocessor {
    command: String,
}

/// Per-host request settings accepted below the `hosts` table.
#[derive(JsonSchema)]
#[schemars(deny_unknown_fields)]
pub(super) struct HostConfig {
    concurrency: Option<usize>,
    request_interval: Option<String>,
    #[schemars(default)]
    headers: HashMap<String, String>,
}

pub(super) fn empty_string_map() -> HashMap<String, String> {
    HashMap::new()
}
