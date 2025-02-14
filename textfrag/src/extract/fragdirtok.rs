use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::ops::Range;

use html5ever::tokenizer::{CharacterTokens, EndTag, NullCharacterToken, StartTag, TagToken};
use html5ever::tokenizer::{ParseError, Token, TokenSink, TokenSinkResult};
use html5ever::{self, Attribute};

use crate::types::{TextDirective, TextDirectiveKind, TextDirectiveStatus};

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

use crate::types::BlockElementContent;
use crate::utils::{find_first_word, find_last_word};

/// Fragment Directive html5ever Tokenizer
/// This is a TokenSink implementation to process the HTML5 tokens from the
/// website content and check for the presence of the Text Directives
///
/// Block elements are constructed during the tokenization process - nested
/// block elements are supported   
#[derive(Clone, Default)]
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

/// Implement TokenSink for FragmentDirectiveTokenizer
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
                                // info!("new block element (nested inside: {last_block_elt}) - {tag_name}...");
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

                            // info!("ënd of block element {tag_name}");
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
    pub fn new(text_directives: Vec<TextDirective>) -> Self {
        Self {
            recent_block_element: RefCell::new(String::new()),
            block_elements: RefCell::new(Vec::new()),
            content: RefCell::new(BlockElementContent::new()),
            // text_directives: RefCell::new(td_checkers),
            directives: RefCell::new(text_directives),
        }
    }

    pub fn get_text_directives(&self) -> Vec<TextDirective> {
        self.directives.borrow().clone().to_owned()
    }

    /// Check element attributes for visibility field and update the block
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

    fn _check_all_directives(&self) {
        let mut tds = self.text_directives_mut();
        for td in tds.iter_mut() {
            self.check_text_directive(td);
        }
    }

    fn gather_directive_flags(
        search_kind: &TextDirectiveKind,
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

    #[inline(always)]
    fn directives_found(&self, directive: &TextDirective) -> (bool, TextDirectiveKind) {
        let mut next_directive = directive.search_kind().clone();

        let all_directives_found = match directive.search_kind() {
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
                if !directive.suffix().is_empty() {
                    next_directive = TextDirectiveKind::Suffix;
                }
                directive.suffix().is_empty()
            }
            TextDirectiveKind::Suffix => true,
        };

        (all_directives_found, next_directive.clone())
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
                FragmentDirectiveTokenizer::gather_directive_flags(&search_kind, directive);
            let (continue_find, next_directive) = self.directives_found(directive);
            end_directives_loop = continue_find;

            directive.set_status(TextDirectiveStatus::NotFound);

            let start_offset = directive.next_offset(); // td.next_offset.borrow().get();
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
                                if !directive.prefix().is_empty() {
                                    let found_content = self.get_block_content(Some(start..end));

                                    let mut prefix_last_word = "";
                                    if start == start_offset {
                                        prefix_last_word = find_last_word(directive.prefix());
                                    };

                                    let start_first_word = find_first_word(directive.start());
                                    let found_content_first_word = find_first_word(&found_content);

                                    if !format!("{}{}", prefix_last_word, start_first_word)
                                        .escape_default()
                                        .to_string()
                                        .eq(found_content_first_word)
                                    {
                                        log::warn!("content mismatch - looks partial extraction attempted \
                                            {found_content_first_word} vs {prefix_last_word}{start_first_word}");
                                        directive.reset();
                                        directive.set_next_offset(end);
                                        continue;
                                        // return;
                                    }
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
                                if start - start_offset > 1 {
                                    directive.reset();
                                    directive.set_next_offset(end);
                                    continue;
                                }
                                if start == start_offset {
                                    let end_last_word = if !directive.end().is_empty() {
                                        find_last_word(directive.end())
                                    } else {
                                        find_last_word(directive.start())
                                    };
                                    let suffix_first_word = find_first_word(directive.suffix());

                                    let found_content =
                                        self.get_block_content(Some(start_offset..end));
                                    let content_last_word = find_first_word(&found_content);

                                    let word_found =
                                        format!("{}{}", end_last_word, suffix_first_word)
                                            .escape_default()
                                            .to_string();
                                    if !word_found.eq(content_last_word) {
                                        log::warn!("content mismatch - looks partial extraction attempted \
                                           {content_last_word} vs {end_last_word}{suffix_first_word}");
                                        directive.reset();
                                        directive.set_next_offset(end);
                                        continue;
                                    }
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
                            directive.set_status(TextDirectiveStatus::Completed);
                            return;
                        }
                    }
                    TextDirectiveStatus::NotFound => {
                        // We've reached the end of the directive search - let us clean-up the directive
                        // if next_directive == search_kind {
                        //     directive.reset();
                        //     // If we're at the end directive, then let us clear the resultant string too...
                        //     if TextDirectiveKind::End == search_kind {
                        //         directive.clear_result_str();
                        //     }
                        // }

                        // If the directive kind is End, we MIGHT find the end in  some other block element's content -
                        // until then, we keep collecting the block element contents
                        if TextDirectiveKind::End == directive.search_kind() {
                            let end = self.content.borrow().word_count().saturating_sub(1);
                            let range = if end > 0 {
                                Some(start_offset..end)
                            } else {
                                None
                            };

                            let end_content = self.get_block_content(range); // self.content.borrow().get_content(range);
                            directive.append_result_str(&end_content);
                        }

                        // reset the search kind, status and offset fields
                        directive.set_status(TextDirectiveStatus::NotFound);
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

    use crate::types::{FragmentDirective, FragmentDirectiveError, TextDirectiveStatus};

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
        let directive_str = format!(":~:{}", FRAGMENT);

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
            // assert_eq!(results.len(), 1);
            // assert_eq!(results[FRAGMENT], TextDirectiveStatus::Completed);
        }
    }

    #[test]
    fn test_multiple_directives() {
        const FRAGMENT: &str = "text=par-,agraph,inp,-ut&text=and-, some, text";
        let directive_str = format!(":~:{}", FRAGMENT);

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
        let directive_str = format!(":~:{}", FRAGMENT);

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

            // if let Some(err) = results.err() {
            //     match err {
            //         FragmentDirectiveError::PartialOk(v) => {
            //             assert_eq!(v.len(), 2);
            //             assert!(v["text=par-,agraph,inp,-ut"] == TextDirectiveStatus::Completed);
            //             assert!(v["text=and-, some, txt"] == TextDirectiveStatus::NotFound);
            //         }
            //         _ => {}
            //     }
            // }
        }
    }
}
