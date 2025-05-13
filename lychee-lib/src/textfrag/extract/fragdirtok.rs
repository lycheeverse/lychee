//! Fragment Directive html5ever Tokenizer
//!
//! This module defines and implements the `FragmentDirectiveTokenizer` struct, which in turn is
//! html5ever's `TokenSink` implementation. This tokenizer processes HTML5 tokens from website content
//! and check for the presence of text directives.
//!
//! The `FragmentDirectiveTokenizer` constructs block elements during the tokenization process.
//! It supports nested block elements and checks for the presence of text directives within the content.
//!
//! - The tokenizer processes HTML content in a streaming fashion, which allows it to handle large
//!   documents efficiently.
//! - Nested block elements are supported
//! - The visibility of elements is determined based on their attributes (not all cases covered yet).
//!
//! Upon successful finding of all the text directives, the tokenizer will set status to completed and return.
//!
//! The `FragmentDirectiveTokenizer` may encounter the following error conditions:
//! - Invalid HTML content: If the HTML content is malformed, the tokenizer may not be able to process it correctly.
//! - Missing directives: If the specified text directives are not found in the content, the tokenizer will update
//!   and return not found status.
//! - Partial matches: If only few of the text directives are found, the tokenizer will return a partial success error.
//!
//! # Example
//!
//! ```rust
//! use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
//!
//! use lychee_lib::textfrag::extract::fragdirtok::FragmentDirectiveTokenizer;
//! use lychee_lib::textfrag::types::TextDirective;
//! use lychee_lib::textfrag::types::TextDirectiveStatus;
//!
//! fn process_html(content: &str, tokenizer: &FragmentDirectiveTokenizer) {
//!     let tok = Tokenizer::new(
//!                     tokenizer.clone(),
//!                     TokenizerOpts {
//!                         ..Default::default()
//!                     },
//!                 );
//!     let input = BufferQueue::default();
//!     input.pop_front();
//!     input.push_back(content.into());
//!
//!     let _res = tok.feed(&input);
//!     tok.end();
//!
//!     let mut all_directives_ok = true;
//!     for td in &tok.sink.get_text_directives() {
//!         println!("td: {:?}", td.raw_directive());
//!         assert_eq!(td.get_status(), TextDirectiveStatus::Completed);
//!     }
//! }
//!
//! fn main() {
//!     let directives = vec![
//!         TextDirective::from_fragment_as_str("text=a-,paragraph,inline").unwrap(),
//!         TextDirective::from_fragment_as_str("text=para-,graph,example").unwrap(),
//!     ];
//!     let tokenizer = FragmentDirectiveTokenizer::new(directives);
//!
//!     // Tokenize HTML content (example HTML content is provided as a string)
//!     let html_content = r#"
//! <html>
//!     <body>
//!         <p>This is a paragraph with some inline <code>https://example.com</code> and a normal
//!             <a style="display:none;" href="https://example.org">example</a>
//!         </p>
//!     </body>
//! </html>
//! "#;
//!
//!     // Process the HTML content with the tokenizer
//!     process_html(html_content, &tokenizer);
//! }
//! ```
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::ops::Range;

use html5ever::tokenizer::{CharacterTokens, EndTag, NullCharacterToken, StartTag, TagToken};
use html5ever::tokenizer::{ParseError, Token, TokenSink, TokenSinkResult};
use html5ever::{self, Attribute};

use crate::textfrag::types::{TextDirective, TextDirectiveKind, TextDirectiveStatus};

const BLOCK_ELEMENTS: &[&str] = &[
    "ADDRESS",
    "ARTICLE",
    "ASIDE",
    "BLOCKQUOTE",
    "BR",
    "DETAILS",
    "DIALOG",
    "DD",
    "DIV",
    "DL",
    "DT",
    "FIELDSET",
    "FIGCAPTION",
    "FIGURE",
    "FOOTER",
    "FORM",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "HEADER",
    "HGROUP",
    "HR",
    "LI",
    "MAIN",
    "NAV",
    "OL",
    "P",
    "PRE",
    "SECTION",
    "TABLE",
    "UL",
    "TR",
    "TH",
    "TD",
    "COLGROUP",
    "COL",
    "CAPTION",
    "THEAD",
    "TBODY",
    "TFOOT",
];

const _INLINE_ELEMENTS: &[&str] = &[
    "A", "ABBR", "ACRONYM", "B", "BDO", "BIG", "BR", "BUTTON", "CITE", "CODE", "DFN", "EM", "I",
    "IMG", "INPUT", "KBD", "LABEL", "MAP", "OBJECT", "OUTPUT", "Q", "SAMP", "SCRIPT", "SELECT",
    "SMALL", "SPAN", "STRONG", "SUB", "SUP", "TEXTAREA", "TIME", "TT", "VAR",
];

const INVISIBLE_CLAUSES: &[&str] = &["none", "hidden"];
const INVISIBLE_NAMES: &[&str] = &["display", "visibility"];

use crate::textfrag::types::BlockElementContent;
// use crate::textfrag::utils::{find_first_word, find_last_word};

/// Fragment Directive html5ever Tokenizer
/// This is a `TokenSink` implementation to process the HTML5 tokens from the
/// website content and check for the presence of the Text Directives
///
/// Block elements are constructed during the tokenization process - nested
/// block elements are supported   
#[derive(Clone, Default, Debug)]
pub struct FragmentDirectiveTokenizer {
    /// The name of current block element the tokenizer is processing
    recent_block_element: RefCell<String>,
    /// Lists the nested block element names - element is popped when the block ends
    block_elements: RefCell<Vec<String>>,
    /// block element content store
    content: RefCell<BlockElementContent>,
    /// Text Directives list (constructed from the URL's fragment directive)
    pub directives: RefCell<Vec<TextDirective>>,
}

/// Block content access methods
impl FragmentDirectiveTokenizer {
    fn update_block_content(&self, c: char) {
        self.content.borrow_mut().set_content(c);
    }

    fn set_block_start_line(&self, line_number: u64) {
        self.content.borrow().set_start_line(line_number);
    }

    fn set_block_end_line(&self, line_number: u64) {
        self.content.borrow().set_end_line(line_number);
    }

    fn set_block_name(&self, name: &str) {
        self.content.borrow().set_name(name);
    }

    fn get_block_content(&self, range: Option<Range<usize>>) -> String {
        self.content.borrow().get_content(range)
    }

    fn pop_block_element(&self) {
        self.block_elements.borrow_mut().pop();
    }

    fn clear_block_content(&self) {
        self.content.borrow_mut().clear();
    }

    fn _text_directives(&self) -> Ref<'_, Vec<TextDirective>> {
        self.directives.borrow()
    }

    fn text_directives_mut(&self) -> RefMut<'_, Vec<TextDirective>> {
        self.directives.borrow_mut()
    }

    fn find_last_word(content: &str) -> &str {
        content.split_whitespace().last().unwrap_or_default()
    }

    fn find_first_word(content: &str) -> &str {
        content.split_whitespace().next().unwrap_or_default()
    }

    fn find_in_content(
        &self,
        search_str: &str,
        start_offset: usize,
        start_bounded_word: bool,
        end_bounded_word: bool,
        allowed_word_distance: i32,
    ) -> Option<TextDirectiveStatus> {
        self.content.borrow().find(
            search_str,
            start_offset,
            start_bounded_word,
            end_bounded_word,
            allowed_word_distance,
        )
    }
}

/// Implement `TokenSink` for `FragmentDirectiveTokenizer`
impl TokenSink for FragmentDirectiveTokenizer {
    type Handle = ();

    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<Self::Handle> {
        match token {
            CharacterTokens(b) => {
                for c in b.chars() {
                    self.update_block_content(c);
                }
            }
            NullCharacterToken => self.update_block_content('\0'),
            TagToken(tag) => {
                let tag_name = tag.name.to_string().to_uppercase();
                let is_block_element = BLOCK_ELEMENTS.contains(&tag_name.as_str());
                match tag.kind {
                    StartTag => {
                        if is_block_element {
                            // If already a block element is present, this becomes nested block element...
                            // let us process the existing content first
                            if let Some(_last_block_elt) = self.block_elements.borrow().last() {
                                self.check_all_text_directives();
                            }

                            // Insert the block element name into the elements queue and make it as the current active element
                            self.block_elements.borrow_mut().push(tag_name.clone());
                            self.set_active_element(&tag_name);

                            self.set_block_name(&tag_name);
                            self.set_block_start_line(line_number);
                        }
                    }
                    EndTag => {
                        if is_block_element {
                            assert!(self.block_elements.borrow().contains(&tag_name));
                            self.set_block_end_line(line_number);

                            self.check_all_text_directives();

                            // Remove the block element reference from the queue
                            self.pop_block_element();

                            // if this was a nested block element, let us make the current last as the active element
                            if let Some(last_element) = self.block_elements.borrow().last() {
                                self.set_active_element(last_element);
                            } else {
                                self.set_active_element("");
                            }
                        }
                    }
                }
                for attr in &tag.attrs {
                    self.update_element_visibility(attr);
                }

                if tag.self_closing {
                    self.set_block_end_line(line_number);
                }
            }
            ParseError(_err) => {
                self.clear_block_content();
            }
            Token::EOFToken => {
                self.set_block_end_line(line_number);
                self.check_all_text_directives();
            }
            _ => {
                self.clear_block_content();
            }
        }

        TokenSinkResult::Continue
    }
}

impl FragmentDirectiveTokenizer {
    #[must_use]
    /// Construct `FragmentDirectiveTokenizer` using the list of `TextDirective`
    pub const fn new(text_directives: Vec<TextDirective>) -> Self {
        Self {
            recent_block_element: RefCell::new(String::new()),
            block_elements: RefCell::new(Vec::new()),
            content: RefCell::new(BlockElementContent::new()),
            directives: RefCell::new(text_directives),
        }
    }

    /// Returns the list of text directives tokenizer processes
    pub fn get_text_directives(&self) -> Vec<TextDirective> {
        self.directives.borrow().clone().clone()
    }

    /// Check element attributes for visibility field of the `style` and update the block
    /// element visibility flag
    fn update_element_visibility(&self, attr: &Attribute) {
        let local_name = attr.name.local.to_string().to_lowercase();
        if local_name.eq("style") {
            let attr_val = attr.value.to_string();
            assert!(attr_val.find(':').is_some());

            // Gather all the stryle attribute values delimited by ';'
            let style_attrib_map: HashMap<&str, &str> = attr_val
                .split(';')
                .take_while(|s| s.trim().is_empty())
                .map(|attrib| attrib.split_at(attrib.find(':').unwrap()))
                .map(|(k, v)| (k, &v[1..]))
                .collect();

            for sam in style_attrib_map {
                if INVISIBLE_NAMES.contains(&sam.0.to_lowercase().as_str())
                    && INVISIBLE_CLAUSES.contains(&sam.1.to_lowercase().as_str())
                {
                    self.content.borrow_mut().set_visible(false);
                }
            }
        }
    }

    /// active block element
    fn set_active_element(&self, name: &str) {
        let mut e = self.recent_block_element.borrow_mut();
        e.clear();
        e.push_str(name);
    }

    /// Check all the text directives
    fn check_all_text_directives(&self) {
        let mut tds = self.text_directives_mut();
        for td in tds.iter_mut() {
            if TextDirectiveStatus::Completed != td.get_status() {
                self.check_text_directive(td);
            }
        }

        // Time to clear the block element content
        self.clear_block_content();
    }

    /// From the directive's current search kind, the method identifie and returns the
    /// word boundary conditions (start bounded, end bounded), word distance in which
    /// the directive text is to be found in the content and the `search_string` itself
    fn gather_directive_flags(
        search_kind: TextDirectiveKind,
        directive: &TextDirective,
    ) -> (bool, bool, i32, String) {
        let mut start_bounded_word = false;
        let mut end_bounded_word = false;
        let mut max_word_distance = -1;

        let search_str = match search_kind {
            TextDirectiveKind::Prefix => {
                start_bounded_word = true;
                // end_bounded_word = true;
                directive.prefix()
            }
            TextDirectiveKind::Start => {
                if directive.prefix().is_empty() {
                    start_bounded_word = true;
                } else {
                    max_word_distance = 1;
                }

                if !directive.end().is_empty() || directive.suffix().is_empty() {
                    end_bounded_word = true;
                }
                directive.start()
            }
            TextDirectiveKind::End => {
                start_bounded_word = true;
                if directive.suffix().is_empty() {
                    end_bounded_word = true;
                }
                directive.end()
            }
            TextDirectiveKind::Suffix => {
                end_bounded_word = true;
                max_word_distance = 1;
                directive.suffix()
            }
        };

        (
            start_bounded_word,
            end_bounded_word,
            max_word_distance,
            search_str.to_owned(),
        )
    }

    /// From the given `TextDirective`, gathers if the end of directive search has reached or not.
    /// And if to continue, the medhod identifies the next directive to be searched for in the block
    /// content
    ///
    /// The method returns a tuple of (`end_finding_directive`: bool, kind: `TextDirectiveKind`)
    fn find_next_directives(directive: &TextDirective) -> (bool, TextDirectiveKind) {
        let mut next_directive = directive.search_kind();

        let end_finding_directives = match directive.search_kind() {
            TextDirectiveKind::Prefix => {
                next_directive = TextDirectiveKind::Start;
                false
            }
            TextDirectiveKind::Start => {
                if !directive.end().is_empty() {
                    next_directive = TextDirectiveKind::End;
                } else if !directive.suffix().is_empty() {
                    next_directive = TextDirectiveKind::Suffix;
                }
                directive.end().is_empty() && directive.suffix().is_empty()
            }
            TextDirectiveKind::End => {
                let no_suffix = directive.suffix().is_empty();
                if !no_suffix {
                    next_directive = TextDirectiveKind::Suffix;
                }
                no_suffix
            }
            TextDirectiveKind::Suffix => true,
        };

        (end_finding_directives, next_directive)
    }

    /// Validate the start value found in the block content
    /// start shall start on a word boundary or shall be an end word of the prefix
    ///
    /// This method confirms the correctness of the start word. If the word validation fails,
    /// false is returned, instructing the caller to restart the search from the end offset
    fn validate_start(&self, start: usize, end: usize, directive: &TextDirective) -> bool {
        if !directive.prefix().is_empty() {
            let found_content = self.get_block_content(Some(start..end));

            let mut prefix_last_word = "";
            if start == directive.next_offset() {
                prefix_last_word = FragmentDirectiveTokenizer::find_last_word(directive.prefix());
            };

            let start_first_word = FragmentDirectiveTokenizer::find_first_word(directive.start());
            let found_content_first_word =
                FragmentDirectiveTokenizer::find_first_word(&found_content);

            if !format!("{prefix_last_word}{start_first_word}") //, prefix_last_word, start_first_word)
                .escape_default()
                .to_string()
                .eq(found_content_first_word)
            {
                log::warn!(
                    "content mismatch - looks partial extraction attempted \
                    {found_content_first_word} vs {prefix_last_word}{start_first_word}"
                );
                return false;
            }
        }

        true
    }

    /// Validates the suffix found in the content as suffix shall be a start bounded word
    /// or an end bounded word (of the **last** word of `End` directive)
    fn validate_suffix(&self, start: usize, end: usize, directive: &TextDirective) -> bool {
        let start_offset = directive.next_offset();

        if start == start_offset {
            let end_last_word = if directive.end().is_empty() {
                FragmentDirectiveTokenizer::find_last_word(directive.start())
            } else {
                FragmentDirectiveTokenizer::find_last_word(directive.end())
            };
            let suffix_first_word = FragmentDirectiveTokenizer::find_first_word(directive.suffix());

            let found_content = self.get_block_content(Some(start_offset..end));
            let content_last_word = FragmentDirectiveTokenizer::find_first_word(&found_content);

            let word_found = format!("{end_last_word}{suffix_first_word}") //, end_last_word, suffix_first_word)
                .escape_default()
                .to_string();
            if !word_found.eq(content_last_word) {
                log::warn!(
                    "content mismatch - looks partial extraction attempted \
                   {content_last_word} vs {end_last_word}{suffix_first_word}"
                );
                return false;
            }
        }

        true
    }

    /// Check presence of (each) Text Directive(s) for the current block element content
    /// If all directives are found, return Ok
    /// if only partial directives are found, mark the next directive to be matched with
    /// position information captured and return partial found message
    ///
    fn check_text_directive(&self, directive: &mut TextDirective) {
        let mut end_directives_loop = false;

        while !end_directives_loop {
            let search_kind = directive.search_kind(); //.borrow().clone();

            let (start_bounded_word, end_bounded_word, allowed_word_distance, search_str) =
                FragmentDirectiveTokenizer::gather_directive_flags(search_kind, directive);
            let (end_finding_directives, next_directive) =
                FragmentDirectiveTokenizer::find_next_directives(directive);
            end_directives_loop = end_finding_directives;

            directive.set_status(&TextDirectiveStatus::NotFound);

            let start_offset = directive.next_offset();
            if let Some(status) = self.find_in_content(
                &search_str,
                start_offset,
                start_bounded_word,
                end_bounded_word,
                allowed_word_distance,
            ) {
                match status {
                    TextDirectiveStatus::WordDistanceExceeded(offset) => {
                        directive.reset();
                        directive.set_next_offset(offset);
                        continue;
                    }
                    TextDirectiveStatus::Found((start, end)) => {
                        match search_kind {
                            TextDirectiveKind::Prefix => {}
                            TextDirectiveKind::Start => {
                                if !self.validate_start(start, end, directive) {
                                    directive.reset();
                                    directive.set_next_offset(end + 1);
                                    continue;
                                }

                                directive.append_result_str(&search_str);
                            }
                            TextDirectiveKind::End => {
                                let found_content = self.get_block_content(Some(start_offset..end));
                                directive.append_result_str(&found_content);
                            }
                            TextDirectiveKind::Suffix => {
                                // Suffix MUST be found on the start_offset (or) in the immediate word next to it
                                // **Note:** start is relative to the start offset and hence shall be 0 or 1
                                // any value greater than 1 implies the directive rule was not satisfied!!!
                                if start - start_offset > 1
                                    || !self.validate_suffix(start, end, directive)
                                {
                                    directive.reset();
                                    directive.set_next_offset(end);
                                    continue;
                                }

                                let suffix_replaced_text = directive
                                    .get_result_str_mut()
                                    .replace(directive.suffix(), "");
                                directive.clear_result_str();
                                directive.append_result_str(&suffix_replaced_text);
                            }
                        }

                        // Let us save the end as the next start offset (for Suffix directives)
                        let mut next_offset = end;
                        if end_bounded_word {
                            next_offset += 1;
                        }
                        directive.set_next_offset(next_offset);

                        // We've matched all the text directives...time to exit!
                        if next_directive == search_kind {
                            directive.set_status(&TextDirectiveStatus::Completed);
                            return;
                        }
                    }
                    TextDirectiveStatus::NotFound => {
                        // If the directive kind is End, we MIGHT find the end in  some other block element's content -
                        // until then, we keep collecting the block element contents
                        if TextDirectiveKind::End == directive.search_kind() {
                            let end = self.content.borrow().word_count().saturating_sub(1);
                            let range = if end > 0 {
                                Some(start_offset..end)
                            } else {
                                None
                            };

                            let end_content = self.get_block_content(range);
                            directive.append_result_str(&end_content);
                        }

                        // reset the search kind, status and offset fields
                        directive.set_status(&TextDirectiveStatus::NotFound);
                        directive.reset();
                        return;
                    }
                    TextDirectiveStatus::EndOfContent => {
                        directive.reset();
                        return;
                    }
                    TextDirectiveStatus::NotStarted | TextDirectiveStatus::Completed => {}
                }
            }

            directive.set_search_kind(next_directive);
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::textfrag::types::{FragmentDirective, FragmentDirectiveError, TextDirectiveStatus};

    const HTML_INPUT: &str = "<html>
    <body>
        <p>This is a paragraph with some inline <code>https://example.com</code> and a normal <a style=\"display:none;\" href=\"https://example.org\">example</a></p>
        <pre>
        Some random text
        https://foo.com and http://bar.com/some/path
        Something else
        <a href=\"https://baz.org\">example link inside pre</a>
        And some more random text's prefix is here 
        // Read HTML from standard input
        // let mut chunk = ByteTendril::new();
        // io::stdin().read_to_tendril(&mut chunk).unwrap();
        </pre>
        <p><b>bold</b></p>

        <p>The <abbr title=\"World Health Organization\">\"WHO\"</abbr> was founded in 1948.</p>
    </body>
    </html>";

    #[test]
    fn test_fragment_directive_checker() {
        const FRAGMENT: &str = "text=par-,agraph,inp,-ut";
        let directive_str = format!(":~:{FRAGMENT}");

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            let res = fd.text_directives();
            assert_eq!(res.len(), 1);
            assert_eq!(res[0].prefix(), "par");
            assert_eq!(res[0].start(), "agraph");
            assert_eq!(res[0].end(), "inp");
            assert_eq!(res[0].suffix(), "ut");

            let results = fd.check(HTML_INPUT);
            assert!(results.is_ok());
        }
    }

    #[test]
    fn test_multiple_directives() {
        const FRAGMENT: &str = "text=par-,agraph,inp,-ut&text=and-, some, text";
        let directive_str = format!(":~:{FRAGMENT}");

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            let res = fd.text_directives();
            assert_eq!(res.len(), 2);
            assert_eq!(res[0].prefix(), "par");
            assert_eq!(res[0].start(), "agraph");
            assert_eq!(res[0].end(), "inp");
            assert_eq!(res[0].suffix(), "ut");

            assert_eq!(res[1].prefix(), "and");
            assert_eq!(res[1].start(), "some");
            assert_eq!(res[1].end(), "text");
            assert_eq!(res[1].suffix(), "");

            let results = fd.check(HTML_INPUT);
            assert!(results.is_ok());
        }
    }

    #[test]
    fn test_partial_success() {
        const FRAGMENT: &str = "text=par-,agraph,inp,-ut&text=and-, some, txt";
        let directive_str = format!(":~:{FRAGMENT}",);

        let fd = FragmentDirective::from_fragment_as_str(&directive_str);
        assert!(fd.is_some());

        if let Some(fd) = fd {
            let res = fd.text_directives();
            assert_eq!(res.len(), 2);
            assert_eq!(res[0].prefix(), "par");
            assert_eq!(res[0].start(), "agraph");
            assert_eq!(res[0].end(), "inp");
            assert_eq!(res[0].suffix(), "ut");

            assert_eq!(res[1].prefix(), "and");
            assert_eq!(res[1].start(), "some");
            assert_eq!(res[1].end(), "txt");
            assert_eq!(res[1].suffix(), "");

            let results = fd.check(HTML_INPUT);
            assert!(results.is_err());

            if let Some(FragmentDirectiveError::PartialOk(v)) = results.err() {
                assert_eq!(v.len(), 2);
                assert!(v["text=par-,agraph,inp,-ut"] == TextDirectiveStatus::Completed);
                assert!(v["text=and-, some, txt"] == TextDirectiveStatus::NotFound);
            }
        }
    }
}
