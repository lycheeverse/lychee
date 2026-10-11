//! JSON Schema representations for config values with custom deserializers.
//!
//! These types describe the file format accepted by Serde. They are separate
//! from the runtime types because several runtime types intentionally expose a
//! different command-line representation or live in `lychee-lib`, which should
//! not need to depend on Schemars.

#![allow(dead_code)]

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use std::collections::HashMap;

/// Log levels accepted by the case-insensitive verbosity deserializer.
#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct Verbosity(#[schemars(schema_with = "verbosity_schema")] String);

fn verbosity_schema(_: &mut SchemaGenerator) -> Schema {
    let levels = ["error", "warn", "warning", "info", "debug", "trace"];
    // This looks super wonky, but remember that JSON Schema enums are
    // case-sensitive. So we spell out both ASCII cases so the pattern also
    // works in editors without support for inline regex flags like `(?i)`.
    let alternatives = levels
        .iter()
        .map(|level| {
            level
                .chars()
                // The next line creates a character class for each character, e.g. "info" becomes "[iI][nN][fF][oO]".
                .flat_map(|c| ['[', c, c.to_ascii_uppercase(), ']'])
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("|");
    json_schema!({ "type": "string", "pattern": format!("^({alternatives})$") })
}

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
    #[schemars(schema_with = "status_code_selector_string_schema")] String,
);

#[derive(JsonSchema)]
#[schemars(transparent)]
pub(super) struct StatusRangeString(#[schemars(schema_with = "status_range_string_schema")] String);

fn status_range_pattern() -> String {
    // The parser accepts leading zeros, e.g. "0200" means 200.
    let code = r"0*[1-9][0-9]{2}";
    // An exclusive upper endpoint of 1000 ends at status 999; =1000 is invalid.
    let upper_endpoint = format!("(=?{code}|0*1000)");
    let range = format!(r"({code})?\.\.{upper_endpoint}?");
    // This checks syntax, not endpoint ordering: "500..400" still needs to be
    // rejected by the parser. Encoding that comparison here would obscure the grammar.
    format!("({code}|{range})")
}

fn status_range_string_schema(_: &mut SchemaGenerator) -> Schema {
    let range = status_range_pattern();
    json_schema!({ "type": "string", "pattern": format!("^{range}$") })
}

fn status_code_selector_string_schema(_: &mut SchemaGenerator) -> Schema {
    let range = status_range_pattern();
    let separator = r"\s*,\s*";
    let list = format!("{range}({separator}{range})*");
    // An empty selector is valid; whitespace is allowed around each list item.
    json_schema!({ "type": "string", "pattern": format!(r"^\s*({list})?\s*$") })
}

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
pub(super) struct BasicAuthSelector(
    // lychee currently accepts " example.com user:password " and strips the
    // outer whitespace. That can silently change the password, since a trailing
    // space could be part of it.
    //
    // Changing this behavior could break existing configs, however. For now,
    // allow surrounding whitespace in the schema so editors don't reject an
    // accepted config. If we change the parser, we should update this schema as
    // well.
    #[schemars(regex(pattern = r"^\s*\S+ [^:\s]+:[^:\s]+\s*$"))] String,
);

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
    accept: Option<StatusCodeSelector>,
}

pub(super) fn empty_string_map() -> HashMap<String, String> {
    HashMap::new()
}
