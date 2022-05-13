use regex::Regex;
use reqwest::Url;

/// Remaps allow mapping from a URI pattern to a different URI
///
/// Some use-cases are
/// - Testing URIs prior to production deployment
/// - Testing URIs behind a proxy
///
/// Be careful when using this feature because checking every link against a
/// large set of regular expressions has a performance impact. Also there are no
/// constraints on the URI mapping, so the rules might contradict with each
/// other.
pub type Remaps = Vec<(Regex, Url)>;
