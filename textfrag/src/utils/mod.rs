/// Utility functions for textfrag
#[inline(always)]
pub fn find_last_word(content: &str) -> &str {
    content.split_whitespace().last().unwrap_or_default()
}

#[inline(always)]
pub fn find_first_word(content: &str) -> &str {
    content.split_whitespace().next().unwrap_or_default()
}
