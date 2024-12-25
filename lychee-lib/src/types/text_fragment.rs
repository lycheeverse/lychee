/// Fragment Directive
/// Fragment directives are part of the Url fragment and it follows the Fragment Directive
/// Delimiter ":~:"
/// Fragment Directive is null if the delimiter is not present in the fragment
///
/// Fragment Directive is parsed and processed into individual directives
/// - multiple text directives may appear in the fragment directive
/// - at the moment the specification defines only the Text Directive
///
/// Text Directive
/// Text directives are prefixed by "text=" with the syntax of text directive as
/// text=[prefix-,]start[,end][,-suffix]
/// Every text directive has **start** as MANDATORY while the prefix, end & suffix are optional
/// Refer: `<https://wicg.github.io/scroll-to-text-fragment/#syntax>`
///
use core::str;
use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
use http::StatusCode;
use log::{debug, warn};
use std::str::Utf8Error;
use thiserror::Error;

use percent_encoding::percent_decode_str;
use fancy_regex::Regex;
use url::Url;

use crate::{extract::html::fragdirtok::{CheckerStatus, FragmentDirectiveTokenizer}, types::ErrorKind, Status};

#[derive(Debug, Error, PartialEq)]
pub enum TextFragmentStatus {
    #[error("Fragment Directive delimiter missing")]
    FragmentDirectiveDelimiterMissing,

    #[error("Not a Text Directive")]
    NotTextDirective,

    #[error("Regex no capture error for directive: {0}")]
    RegexNoCaptureError(String),

    #[error("Start directive is missing error")]
    StartDirectiveMissingError,

    #[error("Percent decode error")]
    PercentDecodeError(Utf8Error),

    #[error("Text directive {0} not found")]
    TextDirectiveNotFound(String),

    #[error("Suffix match error - expected {0} but matched {1}")]
    TextDirectiveRangeError(String, String),

    #[error("Partial text directive match found!")]
    TextDirectivePartialMatchFoundError,

    #[error("Block element is not visible")]
    TextDirectiveBlockElementHidden,
}

/// Text Directive represents the range of text in the web-page for highlighting to the user
/// with the syntax
///     text=[prefix-,]start[,end][,-suffix]
/// **start** is required to be non-null with the other three terms marked as optional.
/// Empty string is NOT valid for all of the directive items
/// **start** with **end** constitutes a text range
/// **prefix** and **suffix** are contextual terms and they are not part of the text fragments to
/// search and gather
///
/// NOTE: directives are percent-encoded by the caller
/// Text Directive will return percent-decoded directives
#[derive(Default, Clone, Debug)]
pub(crate) struct TextDirective {
    /// Prefix directive - a contextual term to help identity text immediately before (the **start**)
    /// OPTIONAL
    pub prefix: String,
    /// Start directive - If only start is given, first instance of the string
    /// specified as **start** is the target
    /// MANDATORY
    pub start: String,
    /// End directive - with this specified, a range of text in the page or input
    /// is referred to be found.
    /// Target text range is the text range starting from **start**, until the first instance
    /// of the **end** (after **start**)
    /// OPTIONAL
    pub end: String,
    /// Suffix directive - a contextual term to identify the text immediately after (the *end*)
    /// OPTIONAL
    pub suffix: String,
    /// Text Directive fragment source(for reference)
    pub raw_directive: String,
}

pub(crate) const FRAGMENT_DIRECTIVE_DELIMITER: &str = ":~:";
pub(crate) const TEXT_DIRECTIVE_DELIMITER: &str = "text=";

pub(crate) const TEXT_DIRECTIVE_REGEX: &str = r"(?s)^text=(?:\s*(?P<prefix>[^,&-]*)-\s*[,$]?\s*)?(?:\s*(?P<start>[^-&,]*)\s*)(?:\s*,\s*(?P<end>[^,&-]*)\s*)?(?:\s*,\s*-(?P<suffix>[^,&-]*)\s*)?$";

impl TextDirective {
    fn percent_decode(input: &str) -> Result<String, ErrorKind> {
        let decode = percent_decode_str(input).decode_utf8();

        match decode {
            Ok(decode) => Ok(decode.escape_default().to_string()),
            Err(e) => Err(ErrorKind::TextFragmentError(
                TextFragmentStatus::PercentDecodeError(e),
            )),
        }
    }

    /// Extract `TextDirective` from fragment string
    /// 
    /// Text directives are percent encoded and we'll decode after extracting
    /// 
    /// Start is MANDATORY field - cannot be empty
    /// end, prefix & suffix are optional
    /// 
    /// # Errors
    /// - `TextFragmentError::NotTextDirective`, if delimiter (text=) is missing
    /// - `TextFragmentError::RegexNoCaptureError`, if the regex capture returns empty
    /// - `TextFragmentError::StartDirectiveMissingError`, if the **start** is missing in the directives
    /// - `TextFragmentError::PercentDecodeError`, if the percent decode fails for the directive
    ///
    fn from_fragment_as_str(fragment: &str) -> Result<TextDirective, ErrorKind> {
        // If text directive delimiter (text=) is not found, return error
        if !fragment.contains(TEXT_DIRECTIVE_DELIMITER) {
            return Err(ErrorKind::TextFragmentError(
                TextFragmentStatus::NotTextDirective,
            ))
        }

        let regex = Regex::new(TEXT_DIRECTIVE_REGEX).unwrap();

        // let result = regex.captures(&fragment).unwrap();
        let result = match regex.captures(fragment) {
            Ok(Some(result)) => result,
            Ok(None)  => {
                return Err(ErrorKind::TextFragmentError(
                    TextFragmentStatus::RegexNoCaptureError(fragment.to_string()),
                ));
            },
            Err(e) => {
                return Err(ErrorKind::TextFragmentError(
                    TextFragmentStatus::RegexNoCaptureError(e.to_string()),
                ));
            }
        };

        let start = result
            .name("start")
            .map(|start| start.as_str())
            .unwrap_or_default();
        let start = TextDirective::percent_decode(start)?;

        // Start is MANDATORY - check for valid directive input
        if start.is_empty() {
            return Err(ErrorKind::TextFragmentError(
                TextFragmentStatus::StartDirectiveMissingError,
            ));
        }

        let end = result
            .name("end")
            .map(|e|e.as_str())
            .unwrap_or_default();
        let end = TextDirective::percent_decode(end)?;

        let prefix = result
            .name("prefix")
            .map(|m| m.as_str())
            .unwrap_or_default();
        let prefix = TextDirective::percent_decode(prefix)?;

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
        })
    }

    fn _check(&self, input: &str) -> Result<Status, ErrorKind> {
        let mut regex = r"(?mi)(?P<selection>".to_string();

        // Construct regex with prefix, start and suffix - as below
        // r"(?mi)(?P<selection>(?<=PREFIX)\sSTART\s(.|\n)+?(?P<last_word>\w+?)\s\b(?=SUFFIX))"
        // last_word shall contain the END directive, if match found - this will be confirmed for range checking!
        if !self.prefix.is_empty() {
            regex.push_str(&format!(r"(?<={})\s", self.prefix.as_str()));
        }

        assert!(!self.start.is_empty());
        regex.push_str(&self.start.clone());

        // START AND END, without SUFFIX
        if self.suffix.is_empty() && !self.end.is_empty() {
            regex.push_str(&format!(r"\s(.|\n)+?\b{}", self.end));
        }

        if !self.suffix.is_empty() {
            regex.push_str(&format!(r"(.|\n)+?(?P<last_word>\w+?)\s\b(?={})", self.suffix));
        }

        // regex.push_str(r")");
        regex.push(')');

        let debug = true;

        if debug {
            println!("regex_str: {regex}");
        }

        let regex = Regex::new(&regex);
        let captures = match regex {
            // Ok(regex) => regex.captures(input)?.unwrap().unwrap(),
            Ok(regex) => {
                if let Ok(Some(captures)) = regex.captures(input) {
                    captures
                } else {
                    return Err(ErrorKind::TextFragmentError(TextFragmentStatus::RegexNoCaptureError(self.raw_directive.clone())));
                }
            }
            Err(_e) => {
                return Err(ErrorKind::TextFragmentError(
                    TextFragmentStatus::TextDirectiveNotFound(self.raw_directive.clone()),
                ))
            }
        };

        if debug {
            let regex_match = captures.name("selection");

            if let Some(m) = regex_match {
                let selection = m.as_str();
                println!("selected text: {selection}");

            }
        }

        // If suffix is given, the regex will not include END explicitly but is checked as the 
        // **last_word** from the regex capture
        if !self.suffix.is_empty() && !self.end.is_empty() {
            let end_lowercase = self.end.clone().to_lowercase();
            let lastword_lowercase = match captures.name("last_word") {
                Some(lw) => {
                    lw.as_str().to_lowercase()
                },
                None => {
                    return Err(ErrorKind::TextFragmentError(TextFragmentStatus::TextDirectiveNotFound(self.raw_directive.clone())))
                }
            };

            // let lastword_lowercase = captures.name("last_word").unwrap().as_str();
            // let lastword_lowercase = lastword_lowercase.to_lowercase();

            if !end_lowercase.eq(&lastword_lowercase) {
                return Err(ErrorKind::TextFragmentError(TextFragmentStatus::TextDirectiveRangeError(end_lowercase, lastword_lowercase)));
            }
        }

        Ok(Status::Ok(StatusCode::OK))
    }
}

#[derive(Default, Clone, Debug)]
pub(crate) struct FragmentDirective {
    #[allow(dead_code)]
    url: Option<Url>,
    pub(crate) text_directives: Vec<TextDirective>,
}

impl FragmentDirective {
    // Extract Text Directives, from the fragment directive
    fn extract_text_directives(fragment: &str) -> Option<Vec<TextDirective>> {
        let mut text_directives = Vec::new();

        // Find the start of the fragment directive delimiter
        if let Some(offset) = fragment.find(FRAGMENT_DIRECTIVE_DELIMITER) {
            let s: &str = &fragment[offset + FRAGMENT_DIRECTIVE_DELIMITER.len()..];
            for td in s.split('&').enumerate() {
                let text_directive = TextDirective::from_fragment_as_str(td.1);
                if let Ok(text_directive) = text_directive {
                    text_directives.push(text_directive);
                }
                // else {
                //     text_directive.inspect_err(|e|eprintln!("{}", e));
                // }
            }

            return Some(text_directives);
        } 

        // WARN: <log>
        warn!("Not a fragment directive!");
        None
    }

    /// Extract Fragment Directive from the (fragment) string as input
    /// Returns a list of the Text Directives from the fragment string
    pub(crate) fn from_fragment_as_str(fragment: &str) -> Option<FragmentDirective> {
        FragmentDirective::extract_text_directives(fragment).map(|text_directives| Self { text_directives, url: None })
    }

    /// Find the Fragment Directive from the Url
    /// If the fragment is not found, return None
    pub(crate) fn from_url(url: &Url) -> Option<FragmentDirective> {
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
    ///
    /// 
    pub(crate) fn check(&self, input: &str) -> Result<Status, ErrorKind> {
        self.check_fragment_directive(input)
    }

    /// Fragment Directive checker method - takes website response text and text directives 
    /// as input and returns Directive check status (as HTTP Status)
    /// 
    /// # Errors
    /// - `TextDirectiveNotFound`, if text directive match fails
    fn check_fragment_directive(&self, buf: &str)  -> Result<Status, ErrorKind> {
        let fd_checker = FragmentDirectiveTokenizer::new(self.text_directives.clone());

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

        let mut error = None;

        for td in &tok.sink.get_text_directive_checkers() {
            let status = td.get_status();
            debug!("tdirective: {:?}", td.get_text_directive());
            debug!("status: {:?}", status);
            debug!("result: {:?}", td.get_result_str());

            if status != CheckerStatus::Completed {
                error = Some(ErrorKind::TextFragmentError(
                    TextFragmentStatus::TextDirectiveNotFound(td.get_text_directive()))
                );
            }
        }

        if let Some(error) = error {
            return Err(error);
        }

        Ok(Status::Ok(StatusCode::OK))
    }

}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use url::Url;

    use crate::{utils::url::UrlExt, Status};

    use super::FragmentDirective;

    const TEST_FRAGMENT: &str = ":~:text=prefix-,start,end,-suffix&text=start,-suffix%2Dwith%2Ddashes&unknown_directive&text=prefix%2Donly-";

    fn print_fragment_directive(fd: &Option<FragmentDirective>) {
        match fd {
            Some(fd) => {
                println!("url: {:?}", fd.url);
                for td in fd.text_directives.clone().into_iter().enumerate() {
                    println!(
                        "{}. for fragment directive - {}, Text Directive is:\n\tprefix: {:?}, start: {:?}, end: {:?}, suffix: {:?}",
                        td.0, td.1.raw_directive, td.1.prefix, td.1.start, td.1.end, td.1.suffix
                    );
                }
            }
            _ => {}
        }
    }

    const MULTILINE_INPUT: &str = 
    "Is there a way to deal with repeated instances of this split in a block of text? FOr instance: \"This is just\na simple sentence. Here is some additional stuff. This is just\na simple sentence. And here is some more stuff. This is just\na simple sentence. \". Currently it matches the entire string, rather than and therefore each instance. prefix   
    
    start (immediately after the prefix and this) is start the statement and continue till the end. the suffix shall come into effect as well.there is going to be a test for starting is mapped or not.
    
    actual end is this new line.
    
    type
    Hints at the linked URL's format with a MIME type. No built-in functionality.

    ";
    
    #[test]
    fn test_fragment_directive_start_only() {
        const FRAGMENT: &str = ":~:text=repeated";
        println!("as_str...{:#?}", FRAGMENT);

        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);
        print_fragment_directive(&fd);

        match fd {
            Some(fd) => {
                match fd.check(&MULTILINE_INPUT) {
                    Ok(status) => {
                        println!("Fragment directive found {}!", status);
                        assert_eq!(status, Status::Ok(StatusCode::OK));
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            None => {},
        };
    }

    #[test]
    fn test_fragment_directive_start_end() {
        const FRAGMENT: &str = ":~:text=repeated, block";
        println!("as_str...{:#?}", FRAGMENT);

        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);
        print_fragment_directive(&fd);

        match fd {
            Some(fd) => {
                match fd.check(&MULTILINE_INPUT) {
                    Ok(status) => {
                        println!("Fragment directive found {}!", status);
                        assert_eq!(status, Status::Ok(StatusCode::OK));
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            None => {},
        };
    }

    #[test]
    fn test_fragment_directive_prefix_start() {
        const FRAGMENT: &str = ":~:text=with-,repeated";
        println!("as_str...{:#?}", FRAGMENT);

        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);
        print_fragment_directive(&fd);

        match fd {
            Some(fd) => {
                match fd.check(&MULTILINE_INPUT) {
                    Ok(status) => {
                        println!("Fragment directive found {}!", status);
                        assert_eq!(status, Status::Ok(StatusCode::OK));
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            None => {},
        };
    }

    #[test]
    fn test_fragment_directive_start_suffix() {
        const FRAGMENT: &str = ":~:text=linked%20URL,-'s%20format";
        println!("as_str...{:#?}", FRAGMENT);

        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);
        print_fragment_directive(&fd);

        match fd {
            Some(fd) => {
                match fd.check(&MULTILINE_INPUT) {
                    Ok(status) => {
                        println!("Fragment directive found {}!", status);
                        assert_eq!(status, Status::Ok(StatusCode::OK));
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            None => {},
        };
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix() {
        const FRAGMENT: &str = ":~:text=with-,repeated,-instance";
        println!("as_str...{:#?}", FRAGMENT);

        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);
        print_fragment_directive(&fd);

        match fd {
            Some(fd) => {
                match fd.check(&MULTILINE_INPUT) {
                    Ok(status) => {
                        println!("Fragment directive found {}!", status);
                        assert_eq!(status, Status::Ok(StatusCode::OK));
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            None => {},
        };
    }

    #[test]
    fn test_fragment_directive_prefix_start_suffix_end() {
        const FRAGMENT: &str = ":~:text=with-,repeated, For, -instance";
        println!("as_str...{:#?}", FRAGMENT);

        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);
        print_fragment_directive(&fd);

        match fd {
            Some(fd) => {
                match fd.check(&MULTILINE_INPUT) {
                    Ok(status) => {
                        println!("Fragment directive found {}!", status);
                        assert_eq!(status, Status::Ok(StatusCode::OK));
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            None => {},
        };
    }

    #[test]
    fn test_fragment_directive_as_url() {
        let url = Url::parse(&("https://example.com/#test".to_owned() + TEST_FRAGMENT)).unwrap();

        let fd = FragmentDirective::from_url(&url);

        println!("as_url...{:?}", TEST_FRAGMENT);
        print_fragment_directive(&fd);

        println!("--- FROM Url.fragment_text_directives() ----");
        let fd = url.fragment_directive();
        print_fragment_directive(&fd);
    }
}
