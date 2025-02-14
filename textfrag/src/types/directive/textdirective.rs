/// Text Directive struct and its support functions
use std::cell::{Cell, RefCell, RefMut};

use fancy_regex::Regex;
use percent_encoding::percent_decode_str;

use crate::types::{
    directive::TextDirectiveKind,
    error::TextFragmentError,
    status::{FragmentDirectiveStatus, TextDirectiveStatus},
};

/// Text Directive represents the range of text in the web-page for highlighting to the user
/// with the syntax
///     text=[prefix-,]start[,end][,-suffix]
/// *start* is required to be non-null with the other three terms marked as optional.
/// Empty string is NOT valid for all of the directive items
/// *start* with *end* constitutes a text range
/// *prefix* and *suffix* are contextual terms and they are not part of the text fragments to
/// search and gather
///
/// NOTE: directives are percent-encoded by the caller
/// Text Directive will return percent-decoded directives
#[derive(Default, Clone, Debug)]
pub struct TextDirective {
    /// Prefix directive - a contextual term to help identity text immediately before (the *start*)
    /// the directive ends with a hyphen (-) to separate from the *start* term
    /// starts on the word boundary
    /// OPTIONAL
    prefix: String,
    /// Start directive - If only start is given, first instance of the string
    /// specified as *start* is the target
    /// MANDATORY
    start: String,
    /// End directive - with this specified, a range of text in the page or block content
    /// is to be found.
    /// Target text range startis from *start*, until the first instance
    /// of the *end* (after *start*)
    /// OPTIONAL
    end: String,
    /// Suffix directive - a contextual term to identify the text immediately after (the *end*)
    /// ends with a hyphen (-) to separate from the *end* term
    /// OPTIONAL
    suffix: String,
    #[allow(dead_code)]
    /// Text Directive fragment source(for reference)
    raw_directive: String,

    // For Tokenizer state machine...
    /// Text Directive validation Status - updated by the tokenizer state machine
    status: RefCell<TextDirectiveStatus>,
    /// Current search string - this will be dynamically updated by the tokenizer state machine
    search_kind: RefCell<TextDirectiveKind>,
    /// start offset to start searching **search_str** on the block element content
    next_offset: Cell<usize>,
    /// Tokenizer resultant string
    resultant_str: RefCell<String>,
}

pub(crate) const TEXT_DIRECTIVE_DELIMITER: &str = "text=";

pub(crate) const TEXT_DIRECTIVE_REGEX: &str = r"(?s)^text=(?:\s*(?P<prefix>[^,&-]*)-\s*[,$]?\s*)?(?:\s*(?P<start>[^-&,]*)\s*)(?:\s*,\s*(?P<end>[^,&-]*)\s*)?(?:\s*,\s*-(?P<suffix>[^,&-]*)\s*)?$";

/// Text Directive getters and setters
impl TextDirective {
    pub fn search_kind(&self) -> TextDirectiveKind {
        self.search_kind.borrow().to_owned()
    }

    pub fn set_search_kind(&self, kind: TextDirectiveKind) {
        *self.search_kind.borrow_mut() = kind;
    }

    pub fn next_offset(&self) -> usize {
        self.next_offset.get()
    }

    pub fn set_next_offset(&self, offset: usize) {
        self.next_offset.set(offset);
    }

    pub fn reset(&self) {
        // reset the search kind, and offset fields
        self.set_next_offset(0);
        self.set_status(TextDirectiveStatus::NotFound);

        // End directive can span across blocks (rest other directives MUST be on the same block)
        // If the next directive is End, we retain the resultant string found so far
        if TextDirectiveKind::End != self.search_kind() {
            self.resultant_str.borrow_mut().clear();

            // Restart the search
            *self.search_kind.borrow_mut() = TextDirectiveKind::Start;
            if !self.prefix().is_empty() {
                *self.search_kind.borrow_mut() = TextDirectiveKind::Prefix;
            }
        }
    }

    /// Update resultant string content (padding with whitespace, for readability)
    pub fn append_result_str(&self, content: &str) {
        let mut res_str = self.resultant_str.borrow_mut();

        if !res_str.is_empty() {
            res_str.push(' ');
        }
        res_str.push_str(content);
    }

    pub fn clear_result_str(&self) {
        self.resultant_str.borrow_mut().clear();
    }

    pub fn get_result_str(&self) -> String {
        self.resultant_str.borrow().to_string()
    }

    pub fn get_result_str_mut(&self) -> RefMut<String> {
        self.resultant_str.borrow_mut()
    }

    pub fn get_status(&self) -> TextDirectiveStatus {
        self.status.borrow().clone()
    }

    pub fn set_status(&self, status: TextDirectiveStatus) {
        *self.status.borrow_mut() = status.clone();
    }

    /// Return the raw text directive
    pub fn get_text_directive(&self) -> String {
        self.raw_directive().to_owned()
    }
    pub fn prefix(&self) -> &str {
        self.prefix.as_str()
    }

    pub fn start(&self) -> &str {
        self.start.as_str()
    }

    pub fn end(&self) -> &str {
        self.end.as_str()
    }

    pub fn suffix(&self) -> &str {
        self.suffix.as_str()
    }

    pub fn raw_directive(&self) -> &str {
        self.raw_directive.as_str()
    }
}

/// Text Directive construction and validation
impl TextDirective {
    /// Percent decode the input string
    /// Returns the decoded string or error
    /// # Errors
    /// - `TextFragmentError::PercentDecodeError`, if the percent decode fails
    fn percent_decode(input: &str) -> Result<String, TextFragmentError> {
        let decode = percent_decode_str(input).decode_utf8();

        match decode {
            Ok(decode) => Ok(decode.to_string()),
            Err(e) => Err(TextFragmentError::PercentDecodeError(e.to_string())),
        }
    }

    /// Extract `TextDirective` from fragment string
    ///
    /// Text directives are percent encoded; we'll extract the directives first
    /// and will decode the extracted directives
    ///
    /// Start is MANDATORY field - cannot be empty
    /// end, prefix & suffix are optional
    ///
    /// # Errors
    /// - `TextFragmentError::NotTextDirective`, if delimiter (text=) is missing
    /// - `TextFragmentError::RegexNoCaptureError`, if the regex capture returns empty
    /// - `TextFragmentError::StartDirectiveMissingError`, if the *start* is missing in the directives
    /// - `TextFragmentError::PercentDecodeError`, if the percent decode fails for the directive
    ///
    pub fn from_fragment_as_str(fragment: &str) -> Result<TextDirective, TextFragmentError> {
        // If text directive delimiter (text=) is not found, return error
        if !fragment.contains(TEXT_DIRECTIVE_DELIMITER) {
            return Err(TextFragmentError::NotTextDirective);
        }

        // XXX: we anticipate no issues in constructing the regex - otherwise, let there be panic!
        let regex = Regex::new(TEXT_DIRECTIVE_REGEX);
        if regex.is_err() {
            log::error!(
                "Error constructing the regex object: {}",
                TEXT_DIRECTIVE_REGEX
            );
            return Err(TextFragmentError::RegexConsructionError(
                TEXT_DIRECTIVE_REGEX.to_string(),
            ));
        }

        let regex = regex.unwrap();
        if let Ok(Some(result)) = regex.captures(fragment) {
            let start = result
                .name("start")
                .map(|start| start.as_str())
                .unwrap_or_default();
            let start = TextDirective::percent_decode(start)?;

            // Start is MANDATORY - check for valid directive input
            if start.is_empty() {
                return Err(TextFragmentError::StartDirectiveMissingError);
            }

            let mut search_kind = TextDirectiveKind::Start;

            let prefix = result
                .name("prefix")
                .map(|m| m.as_str())
                .unwrap_or_default();
            let prefix = TextDirective::percent_decode(prefix)?;
            if !prefix.is_empty() {
                search_kind = TextDirectiveKind::Prefix;
            }

            let end = result.name("end").map(|e| e.as_str()).unwrap_or_default();
            let end = TextDirective::percent_decode(end)?;

            let suffix = result
                .name("suffix")
                .map(|m| m.as_str())
                .unwrap_or_default();
            let suffix = TextDirective::percent_decode(suffix)?;

            Ok(TextDirective {
                prefix,
                start,
                end,
                suffix,
                raw_directive: fragment.to_owned(),
                status: RefCell::new(TextDirectiveStatus::NotStarted),
                search_kind: search_kind.into(),
                next_offset: Cell::new(0),
                resultant_str: RefCell::new(String::new()),
            })
        } else {
            Err(TextFragmentError::RegexCaptureError(
                fragment.to_string(),
                TEXT_DIRECTIVE_REGEX.to_string(),
            ))
        }
    }

    /// [Internal] the use of regular expression does not comply with the specification
    /// To be used for testing purposes only
    fn _check(&self, input: &str) -> Result<FragmentDirectiveStatus, TextFragmentError> {
        let mut s_regex = r"(?mi)(?P<selection>".to_string();

        // Construct regex with prefix, start and suffix - as below
        // r"(?mi)(?P<selection>(?<=PREFIX)\sSTART\s(.|\n)+?(?P<last_word>\w+?)\s\b(?=SUFFIX))"
        // last_word shall contain the END directive, if match found - this will be confirmed for range checking!
        if !self.prefix.is_empty() {
            s_regex.push_str(&format!(r"(?<={})\s", self.prefix.as_str()));
        }

        assert!(!self.start.is_empty());
        s_regex.push_str(&self.start.clone());

        // START AND END, without SUFFIX
        if self.suffix.is_empty() && !self.end.is_empty() {
            s_regex.push_str(&format!(r"\s(.|\n)+?\b{}", self.end));
        }

        if !self.suffix.is_empty() {
            s_regex.push_str(&format!(
                r"(.|\n)+?(?P<last_word>\w+?)\s\b(?={})",
                self.suffix
            ));
        }

        s_regex.push(')');
        log::debug!("regex_str: {s_regex}");

        let captures = if let Ok(regex) = Regex::new(&s_regex) {
            if let Ok(captures) = regex.captures(input) {
                captures
            } else {
                return Err(TextFragmentError::RegexCaptureError(
                    self.raw_directive.clone(),
                    s_regex.clone(),
                ));
            }
        } else {
            return Err(TextFragmentError::RegexConsructionError(s_regex.clone()));
        };

        if let Some(captures) = captures {
            let selection = captures.name("selection");

            if let Some(m) = selection {
                let selection = m.as_str();
                log::debug!("selected text: {selection}");
            }

            // If suffix is given, the regex will not include END explicitly but is checked as the
            // *last_word* from the regex capture
            if !self.suffix.is_empty() && !self.end.is_empty() {
                let end_lowercase = self.end.clone().to_lowercase();
                let lastword_lowercase = match captures.name("last_word") {
                    Some(lw) => lw.as_str().to_lowercase(),
                    None => {
                        return Err(TextFragmentError::TextDirectiveNotFound(
                            self.raw_directive.clone(),
                        ))
                    }
                };

                if !end_lowercase.eq(&lastword_lowercase) {
                    return Err(TextFragmentError::TextDirectiveRangeError(
                        end_lowercase,
                        lastword_lowercase,
                    ));
                }
            }
        }

        Ok(FragmentDirectiveStatus::Ok)
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{TextDirective, TextDirectiveKind, TextFragmentError};

    #[test]
    fn test_fragment_directive_start_only() {
        const FRAGMENT: &str = "text=repeated";

        let td = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(td.is_ok());
        assert_eq!(td.clone().unwrap().start(), "repeated");
        assert_eq!(td.clone().unwrap().search_kind(), TextDirectiveKind::Start);
    }

    #[test]
    fn test_fragment_directive_start_end() {
        const FRAGMENT: &str = "text=repeated, block";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(tdirective.is_ok());
        assert_eq!(tdirective.clone().unwrap().start(), "repeated");
        assert_eq!(tdirective.clone().unwrap().end(), "block");
    }

    #[test]
    fn test_fragment_directive_prefix_start() {
        const FRAGMENT: &str = "text=with-,repeated";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(tdirective.is_ok());
        assert_eq!(tdirective.clone().unwrap().prefix(), "with");
        assert_eq!(tdirective.clone().unwrap().start(), "repeated");
        assert_eq!(
            tdirective.clone().unwrap().search_kind(),
            TextDirectiveKind::Prefix
        );
    }

    #[test]
    fn test_fragment_directive_start_suffix() {
        const FRAGMENT: &str = "text=linked%20URL,-'s%20format";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(tdirective.is_ok());
        assert_eq!(tdirective.clone().unwrap().start(), "linked URL");
        assert_eq!(tdirective.clone().unwrap().suffix(), "'s format");
        assert_eq!(
            tdirective.clone().unwrap().search_kind(),
            TextDirectiveKind::Start
        );
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix() {
        const FRAGMENT: &str = "text=with-,repeated,-instance";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(tdirective.is_ok());
        assert_eq!(tdirective.clone().unwrap().prefix(), "with");
        assert_eq!(tdirective.clone().unwrap().start(), "repeated");
        assert_eq!(tdirective.clone().unwrap().suffix(), "instance");
        assert_eq!(
            tdirective.clone().unwrap().search_kind(),
            TextDirectiveKind::Prefix
        );
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix_end() {
        const FRAGMENT: &str = "text=with-,repeated, For, -instance";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(tdirective.is_ok());
        assert_eq!(tdirective.clone().unwrap().prefix(), "with");
        assert_eq!(tdirective.clone().unwrap().start(), "repeated");
        assert_eq!(tdirective.clone().unwrap().suffix(), "instance");
        assert_eq!(tdirective.clone().unwrap().end(), "For");
        assert_eq!(
            tdirective.clone().unwrap().search_kind(),
            TextDirectiveKind::Prefix
        );
    }

    #[test]
    fn test_missing_start() {
        const FRAGMENT: &str = "text=suffix-";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(
            tdirective.is_err()
                && tdirective.unwrap_err() == TextFragmentError::StartDirectiveMissingError
        );
    }

    #[test]
    fn test_not_directive() {
        const FRAGMENT: &str = "prefix-";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(
            tdirective.is_err() && tdirective.unwrap_err() == TextFragmentError::NotTextDirective
        );
    }

    #[test]
    fn test_percent_decode_error() {
        const FRAGMENT: &str = "text=with%00%9F%92%96";

        let tdirective = TextDirective::from_fragment_as_str(FRAGMENT);
        assert!(
            tdirective.is_err()
                && tdirective.unwrap_err()
                    == TextFragmentError::PercentDecodeError(
                        "invalid utf-8 sequence of 1 bytes from index 5".to_string()
                    )
        );
    }
}
