//! Fragment Directive object is collection of text fragments in the URL's fragment
//! The delimiter `:~:` is the fragment directive delmiter to separate a list of `[TextDirective]`'s
//! This module defines the functionality to parse, construct and store the `[TextDirective]`'s defined in
//! `[url:Url]`'s fragment.
//!
//! # Example
//!
//! ```rust
//!
//! use url::Url;
//! use lychee_lib::textfrag::types::FragmentDirective;
//!
//! let url = Url::parse("https://example.com/#:~:text=prefix-,start,end,-suffix").unwrap();
//! if let Some(fragment_directive) = FragmentDirective::from_url(&url) {
//!    let directives = fragment_directive.text_directives();
//!    for directive in directives {
//!        println!("Directive: {:?}", directive);
//!    }
//! }
//! ```
use std::{cell::RefCell, collections::HashMap};

use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
use url::Url;

use crate::textfrag::{
    extract::FragmentDirectiveTokenizer,
    types::{
        FragmentDirectiveError, FragmentDirectiveStatus, TextDirective, TextDirectiveStatus,
        TextFragmentError,
    },
};

/// Fragment directive delimiter constant
pub const FRAGMENT_DIRECTIVE_DELIMITER: &str = ":~:";

/// Fragment Directive defines the base url and collection of Text Directives
#[derive(Default, Clone, Debug)]
pub struct FragmentDirective {
    #[allow(dead_code)]
    url: Option<Url>,
    text_directives: RefCell<Vec<TextDirective>>,
}

impl FragmentDirective {
    /// Returns all the processed text directives for the `[url:Url]`
    pub fn text_directives(&self) -> Vec<TextDirective> {
        self.text_directives.borrow().to_owned()
    }

    /// Returns a mutable list of all the processed text directives for the `[url:Url]`
    #[allow(dead_code)]
    pub fn text_directives_mut(&self) -> Vec<TextDirective> {
        self.text_directives.borrow_mut().to_owned()
    }

    /// Extract Text Directives, from the Url fragment string, and return a list
    /// of `TextDirective`'s as vector
    ///
    /// The method supports multiple text directives - each text directive is delimited by `&`.
    /// If the text directive is malformed, the method will skip over it and continue processing
    /// the next text directive.
    ///
    /// # Errors
    /// - `FragmentDirectiveDelimiterMissing` - if the fragment directive delimiter is not found
    ///    in the `[url:Url]`'s fragment
    fn build_text_directives(fragment: &str) -> Result<Vec<TextDirective>, TextFragmentError> {
        // Find the start of the fragment directive delimiter
        if let Some(offset) = fragment.find(FRAGMENT_DIRECTIVE_DELIMITER) {
            let mut text_directives = Vec::new();

            let s: &str = &fragment[offset + FRAGMENT_DIRECTIVE_DELIMITER.len()..];
            for td in s.split('&').enumerate() {
                let text_directive = TextDirective::from_fragment_as_str(td.1);
                if let Ok(text_directive) = text_directive {
                    text_directives.push(text_directive);
                } else {
                    log::warn!(
                        "Failed with error {:?} to parse the text directive: {1}",
                        text_directive.err(),
                        td.1
                    );
                }
            }

            return Ok(text_directives);
        }

        log::warn!("Not a fragment directive!");
        Err(TextFragmentError::FragmentDirectiveDelimiterMissing)
    }

    /// Constructs `FragmentDirective` object, containing a list of `TextDirective`'s
    /// processed from the `[url:Url]`'s fragment string, and returns the object.
    ///
    /// If no fragment directive is found in the `[url:Url]`'s fragment, returns None
    #[must_use]
    pub fn from_fragment_as_str(fragment: &str) -> Option<FragmentDirective> {
        if let Ok(text_directives) = FragmentDirective::build_text_directives(fragment) {
            return Some(Self {
                text_directives: RefCell::new(text_directives),
                url: None,
            });
        };

        None
    }

    /// Finds the Fragment Directive from the Url
    /// If the fragment directive is not found, return None
    #[must_use]
    pub fn from_url(url: &Url) -> Option<FragmentDirective> {
        let fragment = url.fragment()?;
        FragmentDirective::from_fragment_as_str(fragment)
    }

    /// Check the presence of given directive on the (web site response) input
    ///
    /// A fragment directive shall have multiple Text Directives included - the check will validate
    /// each of the text directives
    ///
    /// # Errors
    ///
    /// Return an error if
    /// - No match is found
    /// - Suffix error (spec instructs the fragment SHALL be upto **Suffix** and this error is returned if this condition is violated)
    pub fn check(&self, input: &str) -> Result<FragmentDirectiveStatus, FragmentDirectiveError> {
        self.check_fragment_directive(input)
    }

    /// Fragment Directive checker method - takes website response text and text directives
    /// as input and returns Directive check status (as HTTP Status)
    ///
    /// # Errors
    /// - `TextDirectiveNotFound`, if text directive match fails
    // fn check_fragment_directive(&self, buf: &str) -> Result<TextFragmentStatus, TextFragmentError> {
    fn check_fragment_directive(
        &self,
        buf: &str,
    ) -> Result<FragmentDirectiveStatus, FragmentDirectiveError> {
        let mut map = HashMap::new();
        let fd_checker = FragmentDirectiveTokenizer::new(self.text_directives());

        let tok = Tokenizer::new(
            fd_checker,
            TokenizerOpts {
                ..Default::default()
            },
        );

        let input = BufferQueue::default();
        input.pop_front();
        input.push_back(buf.into());

        let _res = tok.feed(&input);
        tok.end();

        let mut error_count = 0;
        let tds = tok.sink.get_text_directives();
        for td in &tds {
            let directive = td.raw_directive().to_string();
            log::debug!("text directive: {:?}", directive);

            let status = td.get_status();
            if TextDirectiveStatus::Completed != status {
                log::warn!("directive ({:?}) status: {:?}", directive, status);
                error_count += 1;
            }

            let _status = status.to_string();
            log::debug!("search status: {:?}", status);

            let res_str = td.get_result_str();
            log::debug!("search result: {:?}", res_str);

            map.insert(directive, status);
        }

        if error_count > 0 {
            if error_count < tds.len() {
                return Err(FragmentDirectiveError::PartialOk(map.clone()));
            }

            return Err(FragmentDirectiveError::NotFoundError);
        }

        Ok(FragmentDirectiveStatus::Ok)
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::FragmentDirective;
    use crate::textfrag::types::{TextDirectiveKind, UrlExt};

    const TEST_FRAGMENT: &str = ":~:text=prefix-,start,end,-suffix&text=start,-suffix%2Dwith%2Ddashes&unknown_directive&text=prefix%2Donly-";

    const MULTILINE_INPUT: &str = "Is there a way to deal with repeated instances of this split in a block of text? FOr instance:\
     \"This is just\na simple sentence. Here is some additional stuff. This is just\na simple sentence. And here is some more stuff.\
      This is just\na simple sentence. \". Currently it matches the entire string, rather than and therefore each instance. prefix   
    
    start (immediately after the prefix and this) is start the statement and continue till the end. the suffix shall come into effect \
    as well.there is going to be a test for starting is mapped or not.
    
    actual end is this new line.
    
    type
    Hints at the linked URL's format with a MIME type. No built-in functionality.

    ";

    #[test]
    fn test_fragment_directive_start_only() {
        const FRAGMENT: &str = "text=repeated";
        let directive_str = format!(":~:{FRAGMENT}",);

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            assert!(fd.text_directives().len() == 1);
            assert!(
                fd.text_directives()[0].start().eq(&"repeated".to_string())
                    && fd.text_directives()[0].search_kind() == TextDirectiveKind::Start
            );
            assert!(fd.text_directives()[0].prefix().is_empty());

            let results = fd.check(MULTILINE_INPUT);
            assert!(results.is_ok());
        }
    }

    #[test]
    fn test_fragment_directive_start_end() {
        const FRAGMENT: &str = "text=repeated, block";
        let directive_str = format!(":~:{FRAGMENT}",);

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            assert!(fd.text_directives().len() == 1);
            assert_eq!(fd.text_directives()[0].start(), "repeated");
            assert_eq!(fd.text_directives()[0].end(), "block");
            assert_eq!(
                fd.text_directives()[0].search_kind(),
                TextDirectiveKind::Start
            );

            let res = fd.check(MULTILINE_INPUT);
            assert!(res.is_ok());
        }
    }

    #[test]
    fn test_prefix_start_end() {
        const INPUT: &str = r#"
                <html>
                    <body>
                        <p>This is a paragraph with some inline <code>https://example.com</code> and a normal 
                            <a style="display:none;" href="https://example.org">example</a>
                        </p>
                    </body>
                </html>
                "#;
        let text_directive = "text=a-,paragraph,inline";
        let fd_str = format!(":~:{text_directive}",);

        let fd = FragmentDirective::from_fragment_as_str(&fd_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            assert!(fd.text_directives().len() == 1);
            assert_eq!(fd.text_directives()[0].prefix(), "a");
            assert_eq!(fd.text_directives()[0].start(), "paragraph");
            assert_eq!(fd.text_directives()[0].end(), "inline");

            let res = fd.check(INPUT);
            assert!(res.is_ok());
        }
    }

    #[test]
    fn test_fragment_directive_prefix_start() {
        const FRAGMENT: &str = "text=with-,repeated";
        let directive_str = format!(":~:{FRAGMENT}",);

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            assert!(fd.text_directives().len() == 1);
            assert_eq!(fd.text_directives()[0].prefix(), "with");
            assert_eq!(fd.text_directives()[0].start(), "repeated");
            assert_eq!(
                fd.text_directives()[0].search_kind(),
                TextDirectiveKind::Prefix
            );

            let results = fd.check(MULTILINE_INPUT);
            assert!(results.is_ok());
        }
    }

    #[test]
    fn test_fragment_directive_start_suffix() {
        const FRAGMENT: &str = "text=linked%20URL,-'s format";

        let directive_str = format!(":~:{FRAGMENT}",);
        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            assert_eq!(fd.text_directives().len(), 1);
            assert_eq!(fd.text_directives()[0].start(), "linked URL");
            assert_eq!(fd.text_directives()[0].suffix(), "'s format");
            assert_eq!(
                fd.text_directives()[0].search_kind(),
                TextDirectiveKind::Start
            );

            let results = fd.check(MULTILINE_INPUT);
            assert!(results.is_ok());
        };
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix() {
        const FRAGMENT: &str = "text=with-,repeated,-instance";
        let directive_str = format!(":~:{FRAGMENT}",);

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);

        if let Some(fd) = fd {
            assert!(fd.text_directives().len() == 1);
            assert_eq!(fd.text_directives()[0].prefix(), "with");
            assert_eq!(fd.text_directives()[0].start(), "repeated");
            assert_eq!(fd.text_directives()[0].suffix(), "instance");
            assert_eq!(
                fd.text_directives()[0].search_kind(),
                TextDirectiveKind::Prefix
            );

            let results = fd.check(MULTILINE_INPUT);
            assert!(results.is_ok());
        };
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix_end() {
        const FRAGMENT: &str = "text=with-,repeated, mapped, -or";
        let directive_str = format!(":~:{FRAGMENT}",);

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);

        if let Some(fd) = fd {
            assert!(fd.text_directives().len() == 1);
            assert_eq!(fd.text_directives()[0].prefix(), "with");
            assert_eq!(fd.text_directives()[0].start(), "repeated");
            assert_eq!(fd.text_directives()[0].suffix(), "or");
            assert_eq!(fd.text_directives()[0].end(), "mapped");
            assert_eq!(
                fd.text_directives()[0].search_kind(),
                TextDirectiveKind::Prefix
            );

            let results = fd.check(MULTILINE_INPUT);
            assert!(results.is_ok());
        };
    }

    #[test]
    fn test_fragment_directive_as_url() {
        let url = Url::parse(&("https://example.com/#test".to_owned() + TEST_FRAGMENT)).unwrap();
        assert!(url.has_fragment_directive());

        let fd = FragmentDirective::from_url(&url);
        assert!(fd.unwrap().text_directives().len() == 2);

        let fd = url.fragment_directive();
        assert!(fd.unwrap().text_directives().len() == 2);
    }
}
