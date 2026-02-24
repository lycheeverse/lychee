//! Extract URLs from CSS content
//!
//! This module extracts URLs from CSS files and `<style>` tags.
//! It looks for `url()` functions which are commonly used for:
//! - background-image
//! - background
//! - @import statements
//! - font-face src
//! - etc.
// NOTE: this is a regular-expression based extractor and may not cover all edge
// cases of CSS parsing. Specifically, it does not handle escape sequences
// within URLs or nested functions, such as `url("image\"name.png")`.
//
// A more bespoke CSS parser, such as Servo's
// [cssparser](https://github.com/servo/rust-cssparser) crate might or might not
// cover these cases better, but it would come with the additional burden of
// adding multiple dependencies.
//
// For the time being, we accept these limitations, but we may revisit this
// decision in the future if needed.

use std::sync::LazyLock;

use regex::Regex;

use crate::types::uri::raw::{RawUri, SourceSpanProvider, SpanProvider};

/// Regular expression to match CSS `url()` functions
///
/// This regex matches:
/// - url("...")
/// - url('...')
/// - url(...)
///
/// It captures the URL inside the parentheses, handling:
/// - Single quotes
/// - Double quotes
/// - No quotes
/// - Escaped quotes within the URL
///
/// Examples:
/// - `background-image: url("./image.png");`
/// - `background: url('/path/to/image.jpg');`
/// - `@import url(https://example.com/style.css);`
/// - `src: url(../fonts/font.woff2);`
static CSS_URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)                     # Enable extended mode for whitespace and comments
        url\s*\(                    # Match 'url(' with optional whitespace
        \s*                         # Optional whitespace
        (?:                         # Non-capturing group for the URL
            "(?P<double>[^"]*)"     # Double-quoted URL
            |                       # OR
            '(?P<single>[^']*)'     # Single-quoted URL
            |                       # OR
            (?P<unquoted>[^)]+)     # Unquoted URL (anything until ')')
        )
        \s*                         # Optional whitespace
        \)                          # Match closing ')'
        "#,
    )
    .expect("CSS URL regex should be valid")
});

/// Extract all URLs from CSS content
///
/// This function finds all `url()` occurrences in CSS and extracts the URLs.
///
/// # Arguments
///
/// * `input` - The CSS content to extract URLs from
/// * `span_provider` - Provides source location information for extracted URLs
///
/// # Returns
///
/// A vector of `RawUri` objects representing the extracted URLs
///
/// # Examples
///
/// CSS input:
/// ```css
/// .example {
///     background-image: url("./image.png");
///     background: url('/absolute/path.jpg');
/// }
/// @import url(https://example.com/style.css);
/// ```
///
/// Extracts 3 URLs: `./image.png`, `/absolute/path.jpg`, and `https://example.com/style.css`
pub(crate) fn extract_css<S: SpanProvider>(input: &str, span_provider: &S) -> Vec<RawUri> {
    CSS_URL_REGEX
        .captures_iter(input)
        .filter_map(|cap| {
            // Try to extract the URL from any of the three capture groups
            let url = cap
                .name("double")
                .or_else(|| cap.name("single"))
                .or_else(|| cap.name("unquoted"))
                .map(|m| m.as_str().trim())?;

            // Skip empty URLs. Example input: `url("")`
            if url.is_empty() {
                return None;
            }

            // Get the position of the entire match (for span information)
            let match_start = cap.get(0)?.start();

            Some(RawUri {
                text: url.to_string(),
                element: Some("style".to_string()),
                attribute: Some("url".to_string()),
                span: span_provider.span(match_start),
            })
        })
        .collect()
}

/// Extract URLs from CSS content with default span
pub(crate) fn extract_css_with_default_span(input: &str) -> Vec<RawUri> {
    extract_css(input, &SourceSpanProvider::from_input(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests based on MDN documentation:
    // https://developer.mozilla.org/en-US/docs/Web/CSS/url_function

    // Basic usage examples

    #[test]
    fn test_basic_usage_double_quotes() {
        let css = r#"url("https://example.com/images/myImg.jpg");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "https://example.com/images/myImg.jpg");
    }

    #[test]
    fn test_basic_usage_single_quotes() {
        let css = r"url('https://example.com/images/myImg.jpg');";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "https://example.com/images/myImg.jpg");
    }

    #[test]
    fn test_basic_usage_unquoted() {
        let css = r"url(https://example.com/images/myImg.jpg);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "https://example.com/images/myImg.jpg");
    }

    #[test]
    fn test_data_url() {
        let css = r#"url("data:image/jpeg;base64,iRxVB0…");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].text.starts_with("data:image/jpeg"));
    }

    #[test]
    fn test_relative_url() {
        let css = r"url(myImg.jpg);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "myImg.jpg");
    }

    #[test]
    fn test_svg_fragment() {
        let css = r"url(#IDofSVGpath);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "#IDofSVGpath");
    }

    // Associated properties

    #[test]
    fn test_background_image() {
        let css = r#"background-image: url("star.gif");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "star.gif");
    }

    #[test]
    fn test_list_style_image() {
        let css = r"list-style-image: url('../images/bullet.jpg');";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "../images/bullet.jpg");
    }

    #[test]
    fn test_content_property() {
        let css = r#"content: url("my-icon.jpg");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "my-icon.jpg");
    }

    #[test]
    fn test_cursor_property() {
        let css = r"cursor: url(my-cursor.cur);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "my-cursor.cur");
    }

    #[test]
    fn test_border_image_source() {
        let css = r"border-image-source: url(/media/diamonds.png);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "/media/diamonds.png");
    }

    #[test]
    fn test_font_src() {
        let css = r"src: url('fantastic-font.woff');";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "fantastic-font.woff");
    }

    #[test]
    fn test_offset_path() {
        let css = r"offset-path: url(#path);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "#path");
    }

    #[test]
    fn test_mask_image_with_fragment() {
        let css = r#"mask-image: url("masks.svg#mask1");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "masks.svg#mask1");
    }

    // Properties with fallbacks

    #[test]
    fn test_cursor_with_fallback() {
        let css = r"cursor: url(pointer.cur), pointer;";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "pointer.cur");
    }

    // Shorthand properties

    #[test]
    fn test_background_shorthand() {
        let css = r"background: url('star.gif') bottom right repeat-x blue;";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "star.gif");
    }

    #[test]
    fn test_border_image_shorthand() {
        let css = r#"border-image: url("/media/diamonds.png") 30 fill / 30px / 30px space;"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "/media/diamonds.png");
    }

    // As parameter in CSS functions

    #[test]
    fn test_cross_fade_function() {
        let css = r"background-image: cross-fade(20% url(first.png), url(second.png));";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].text, "first.png");
        assert_eq!(urls[1].text, "second.png");
    }

    #[test]
    fn test_image_function() {
        let css =
            r"mask-image: image(url(mask.png), skyblue, linear-gradient(black, transparent));";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "mask.png");
    }

    // Multiple values

    #[test]
    fn test_multiple_urls_in_content() {
        let css =
            r"content: url(star.svg) url(star.svg) url(star.svg) url(star.svg) url(star.svg);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 5);
        for url in &urls {
            assert_eq!(url.text, "star.svg");
        }
    }

    // At-rules

    #[test]
    fn test_document_rule() {
        let css = r#"@document url("https://www.example.com/") { }"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "https://www.example.com/");
    }

    #[test]
    fn test_import_rule() {
        let css = r#"@import url("https://www.example.com/style.css");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "https://www.example.com/style.css");
    }

    #[test]
    fn test_namespace_rule() {
        let css = r"@namespace url(http://www.w3.org/1999/xhtml);";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "http://www.w3.org/1999/xhtml");
    }

    // Complex real-world examples

    #[test]
    fn test_data_url_svg_embedded() {
        let css = r#"background: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='90' height='45'%3E%3Cpath d='M10 10h60' stroke='%2300F' stroke-width='5'/%3E%3Cpath d='M10 20h60' stroke='%230F0' stroke-width='5'/%3E%3Cpath d='M10 30h60' stroke='red' stroke-width='5'/%3E%3C/svg%3E");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].text.starts_with("data:image/svg+xml"));
        assert!(urls[0].text.contains("%3Csvg"));
    }

    #[test]
    fn test_filter_svg_file() {
        let css = r#"filter: url("my-file.svg#svg-blur");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "my-file.svg#svg-blur");
    }

    #[test]
    fn test_filter_svg_inline() {
        let css = r##"filter: url("#svg-blur");"##;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "#svg-blur");
    }

    #[test]
    fn test_extract_multiple_urls() {
        let css = r#"
        .example {
            background-image: url("./image.png");
            background: url('/absolute/path.jpg');
        }
        @import url(https://example.com/style.css);
        @font-face {
            src: url(../fonts/font.woff2);
        }
        "#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 4);
        assert_eq!(urls[0].text, "./image.png");
        assert_eq!(urls[1].text, "/absolute/path.jpg");
        assert_eq!(urls[2].text, "https://example.com/style.css");
        assert_eq!(urls[3].text, "../fonts/font.woff2");
    }

    #[test]
    fn test_extract_url_with_spaces() {
        let css = r#"background: url(  "./image.png"  );"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "./image.png");
    }

    #[test]
    fn test_empty_url() {
        let css = r#"background: url("");"#;
        let urls = extract_css_with_default_span(css);
        // Empty URLs should be skipped
        assert_eq!(urls.len(), 0);
    }

    #[test]
    fn test_no_urls() {
        let css = r"
        .example {
            color: red;
            font-size: 16px;
        }
        ";
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 0);
    }

    #[test]
    fn test_url_in_style_tag_content() {
        // This simulates content that would be inside a <style> tag in HTML
        let css = r#"
        div {
            background-image: url("./lychee.png");
        }
        "#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].text, "./lychee.png");
    }

    #[test]
    fn test_data_url_is_extracted() {
        // Data URLs should still be extracted (even though they might be filtered later)
        let css = r#"background: url("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].text.starts_with("data:image/png"));
    }

    #[test]
    fn test_element_and_attribute_metadata() {
        let css = r#"background: url("./image.png");"#;
        let urls = extract_css_with_default_span(css);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].element, Some("style".to_string()));
        assert_eq!(urls[0].attribute, Some("url".to_string()));
    }
}
