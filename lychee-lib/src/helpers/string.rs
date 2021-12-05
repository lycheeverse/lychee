/// Truncates an input str inplace to `max_len`.
/// Adds an ellipsis if the string got truncated
pub(crate) fn truncate(input: &mut str, max_len: usize) -> String {
    let truncated = input.len() > max_len;
    let mut out = input[..max_len].to_string();
    if truncated {
        out = format!("{}...", out);
    }
    out
}
