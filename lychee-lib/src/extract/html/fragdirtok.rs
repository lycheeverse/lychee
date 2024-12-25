/// Fragment Directive Tokenizer using Html5ever
/// 
use std::ops::Range;
use std::borrow::{self, Borrow, BorrowMut};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use html5ever::{self, Attribute};
use html5ever::tokenizer::{CharacterTokens, EndTag, NullCharacterToken, StartTag, TagToken};
use html5ever::tokenizer::{
    ParseError, Token, TokenSink, TokenSinkResult,
};

use crate::types::TextDirective;

const BLOCK_ELEMENTS: &[&str] = &[
        "ADDRESS", "ARTICLE", "ASIDE", "BLOCKQUOTE", "BR", "DETAILS", "DIALOG", "DD", "DIV", "DL", "DT",
        "FIELDSET", "FIGCAPTION", "FIGURE", "FOOTER", "FORM", "H1", "H2", "H3", "H4", "H5", "H6", "HEADER",
        "HGROUP", "HR", "LI", "MAIN", "NAV", "OL", "P", "PRE", "SECTION", "TABLE", "UL", "TR", "TH", "TD",
        "COLGROUP", "COL", "CAPTION", "THEAD", "TBODY", "TFOOT"
    ];

const _INLINE_ELEMENTS: &[&str] = &[
        "A", "ABBR", "ACRONYM", "B", "BDO", "BIG", "BR", "BUTTON", "CITE", "CODE", "DFN", "EM", "I", "IMG",
        "INPUT", "KBD", "LABEL", "MAP", "OBJECT", "OUTPUT", "Q", "SAMP", "SCRIPT", "SELECT", "SMALL", "SPAN",
        "STRONG", "SUB", "SUP", "TEXTAREA", "TIME", "TT", "VAR"
    ];

const INVISIBLE_CLAUSES: &[&str] = &["none", "hidden"];
const INVISIBLE_NAMES: &[&str] = &["display", "visibility"];

/// Abstract block element content
#[derive(Clone, Default)]
struct BlockElementContent {
    /// Block element name
    element_name: RefCell<String>,
    /// Block starting line number
    start_line_number: Cell<u64>,
    /// Block ending line number
    end_line_number: Cell<u64>,
    /// Block Content
    content: RefCell<String>,
    /// Visibility flag (default is true)
    visible: Cell<bool>,
    /// Indicate if the content is right to left 
    _rtl: Cell<bool>,
    /// Control code - to compute word count (inline)
    new_word: Cell<bool>,
    /// word count
    nwords: Cell<usize>,
}

impl BlockElementContent {
    fn new() -> Self {
        Self {
            element_name: RefCell::new(String::new()),
            content: RefCell::new(String::new()),
            start_line_number: Cell::new(0),
            end_line_number: Cell::new(0),
            visible: Cell::new(true),
            _rtl: Cell::new(false),
            new_word: Cell::new(false),
            nwords: Cell::new(0),
        }
    }

    /// Current block element
    fn set_name(&self, name: &str) {
        let mut elt = self.element_name.borrow_mut();
        if !elt.is_empty() {
            elt.clear();
        }
        elt.push_str(name);
    }

    /// Block Start line number
    fn set_start_line(&self, line_number: u64) {
        self.start_line_number.borrow().set(line_number);
    }

    /// Block ending line number
    fn set_end_line(&self, line_number: u64) {
        self.end_line_number.borrow().set(line_number);
    }

    /// update block content
    fn set_content(&mut self, c: char) {
        // skip the control codes
        if c.is_control() {
            return;
        }

        let mut is_word = false;
        
        if !c.is_whitespace() {
            is_word = true;
        }

        let mut nwords = self.nwords.borrow().get();

        // If previous content input was a whitespace and the current is not, we've a new word
        // increment nwords
        if !self.new_word.borrow().get() && is_word {
            nwords += 1;
        }

        let buf = c.escape_default().collect::<String>(); //.replace("\\n", " ");
        self.content.borrow_mut().push_str(&buf);

        self.nwords.borrow_mut().set(nwords);
        self.new_word.borrow_mut().set(is_word);

    }

    fn word_count(&self) -> usize {
        self.nwords.borrow().get()
    }

    fn get_content_by_range(&self, start: usize, end: usize) -> String {
        let content = self.content.borrow().clone()
                                .split_whitespace()
                                .collect::<Vec<&str>>()[start..=end]
                                .join(" ");
        content
    }

    fn get_content(&self, range: Option<Range<usize>>) -> String {
        if let Some(r) = range {
            self.get_content_by_range(r.start, r.end)
        } else {
            self.content.borrow().clone()
        }
    }

    fn clear(&mut self) {
        self.content.borrow_mut().clear();
        self.new_word.borrow_mut().set(false);
        self.nwords.borrow_mut().set(0);
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible.borrow_mut().set(visible);
    }

    /// Find the `**search_str**` in the block content and, when found, return a pair of
    /// start and end offset on the block content
    /// If content is hidden and/or no match is found, return None
    fn find(&self, search_str: &str, start_offset: usize, start_bounded_word: bool, end_bounded_word: bool) -> Option<CheckerStatus> {
        if !self.visible.borrow().get() {
            return None;
        }

        if self.word_count() < start_offset {
            return Some(CheckerStatus::EndOfContent);
        }

        let c_words = self.content
                                .borrow()
                                .clone();
        let c_words = c_words
                                .split_whitespace()
                                .skip(start_offset)
                                .map(str::to_lowercase)
                                .collect::<Vec<String>>();

        let mut  found = false; 
        let mut start_index = None;
        let mut len = 0;

        let mut u_words_iter = c_words.into_iter().enumerate();
        let swords = search_str
                                .split_whitespace()
                                .map(str::to_lowercase)
                                .collect::<Vec<String>>();

        'çontent_loop: while !found {
            len = 0;
            start_index = None;
            'search_str: for sw in &swords {
                if let Some(uw) = u_words_iter.next() {
                    found = start_bounded_word && end_bounded_word && sw.eq(&uw.1) || 
                            start_bounded_word && uw.1.starts_with(sw) ||
                            end_bounded_word && uw.1.ends_with(sw) ||
                            uw.1.contains(sw);

                    if found {
                        len += 1;
                        if start_index.is_none() {
                            start_index = Some(uw.0);
                        }
                        continue;
                    }

                    // no match - we don't have to process rest of `search_str`
                    break 'search_str;    
                } 

                break 'çontent_loop;
            }
        }

        if found {
            assert!(len == search_str.split_whitespace().collect::<Vec<&str>>().len());

            let start = start_index.unwrap() + start_offset;            
            let end = start + len - 1;
            
            return Some(CheckerStatus::Found((start, end)));
        }
        
        Some(CheckerStatus::NotFound)
    }
}

/// Fragment Directive html5ever Tokenizer
#[derive(Clone, Default)]
pub(crate) struct FragmentDirectiveTokenizer {
    /// The name of current block element the tokenizer is processing
    recent_block_element: RefCell<String>,
    /// Lists the nested block element names - element is popped when the block ends 
    block_elements: RefCell<Vec<String>>,
    /// block element content store
    content: RefCell<BlockElementContent>,
    /// Text Directive Checker (from the URL's fragment directive)
    text_directives: RefCell<Vec<TextDirectiveChecker>>,
}

impl TokenSink for FragmentDirectiveTokenizer {
    type Handle = ();

    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<Self::Handle> {
        match token {
            CharacterTokens(b) => {
                for c in b.chars() {
                    self.content.borrow_mut().set_content(c);
                }
            },
            NullCharacterToken => self.content.borrow_mut().set_content('\0'),
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

                            self.content.borrow().set_name(&tag_name);
                            self.content.borrow().set_start_line(line_number);
                        }
                    },
                    EndTag => {
                        if is_block_element {
                            assert!(self.block_elements.borrow().contains(&tag_name));
                            self.content.borrow().set_end_line(line_number);

                            // info!("ënd of block element {tag_name}");
                            self.check_all_text_directives();

                            // Remove the block element reference from the queue
                            self.block_elements.borrow_mut().pop();

                            // if this was a nested block element, let us make the current last as the active element
                            if let Some(last_element) = self.block_elements.borrow().last() {
                                self.set_active_element(last_element);
                            } else {
                                self.set_active_element("");
                            }
                        }
                    },
                }
                for attr in &tag.attrs {
                    self.update_element_visibility(attr);
                }

                if tag.self_closing {
                    self.content.borrow().set_end_line(line_number);
                }
            },
            ParseError(_err) => {
                self.content.borrow_mut().clear();
            },
            _ => {
                self.content.borrow_mut().clear();
            },
        }

        TokenSinkResult::Continue
    }
}

impl FragmentDirectiveTokenizer {
    pub(crate) fn new(text_directives: Vec<TextDirective>) -> Self {
        let mut td_checkers = Vec::new();
        for td in text_directives {
            let td_checker = TextDirectiveChecker::new(&td);
            td_checkers.push(td_checker);
        }

        Self {
            recent_block_element: RefCell::new(String::new()),
            block_elements: RefCell::new(Vec::new()),
            content: RefCell::new(BlockElementContent::new()),
            text_directives: RefCell::new(td_checkers),
        }
    }

    pub(crate) fn get_text_directive_checkers(&self) -> Vec<TextDirectiveChecker> {
        self.text_directives
        .borrow()
        .iter()
        // .clone()
        .map(borrow::ToOwned::to_owned)
        .collect::<Vec<TextDirectiveChecker>>()
    }

    /// Check element attributes for visibility field and update the block
    /// element visibility flag
    fn update_element_visibility(&self, attr: &Attribute) {
        let local_name = attr.name.local.to_string().to_lowercase();
        if local_name.eq("style") {
            let attr_val = attr.value.to_string();
            assert!(attr_val.find(':').is_some());

            // Gather all the stryle attribute values delimited by ';'
            let style_attrib_map: HashMap<&str, &str> = attr_val.split(';')
                .take_while(|s| s.trim().is_empty())
                .map(|attrib| attrib.split_at(attrib.find(':').unwrap()))
                .map(|(k, v)| (k, &v[1..]))
                .collect();

            for sam in style_attrib_map {
                if INVISIBLE_NAMES.contains(&sam.0.to_lowercase().as_str()) 
                    && INVISIBLE_CLAUSES.contains(&sam.1.to_lowercase().as_str()) {
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
        let mut tds = self.text_directives.borrow_mut();
        for td in tds.iter_mut() {
            if CheckerStatus::Completed != *td.status.borrow() {
                self.check_text_directive(td);
            }
        }

        // Time to clean the block element content
        self.content.borrow_mut().clear();
    }

    fn gather_directive_flags(search_kind: &TextDirectiveKind, directive: &TextDirective) -> (bool, bool, String) {
        let mut start_bounded_word = false;
        let mut end_bounded_word  = false;

        let search_str = match search_kind {
            TextDirectiveKind::Prefix => {
                start_bounded_word = true;
                directive.prefix.clone()
            },
            TextDirectiveKind::Start => {
                if directive.prefix.is_empty() {
                    start_bounded_word = true;
                }

                if !directive.end.is_empty() || directive.suffix.is_empty() {
                    end_bounded_word = true;
                } 
                directive.start.clone()
            },
            TextDirectiveKind::End => {
                start_bounded_word = true;
                if directive.suffix.is_empty() {
                    end_bounded_word = true;
                }                
                directive.end.clone()
            },
            TextDirectiveKind::Suffix => {
                end_bounded_word = true;
                directive.suffix.clone()
            },
        };

        (start_bounded_word, end_bounded_word, search_str)
    }

    /// Check presence of (each) Text Directive(s) for the current block element content
    /// If all directives are found, return Ok 
    /// if only partial directives are found, mark the next directive to be matched with
    /// position information captured and return partial found message
    /// 
    fn check_text_directive(&self, td: &mut TextDirectiveChecker) {
        let mut all_directives_found = false;
        let directive = td.directive.borrow();

        'directive_loop: while !all_directives_found {
            let search_kind = td.search_kind.borrow().clone();

            let mut next_directive = search_kind.clone();
            let (start_bounded_word, end_bounded_word, search_str) = FragmentDirectiveTokenizer::gather_directive_flags(&search_kind, directive);
            all_directives_found = match search_kind {
                TextDirectiveKind::Prefix => {
                    next_directive = TextDirectiveKind::Start;
                    false
                },
                TextDirectiveKind::Start => {
                    if !directive.end.is_empty() {
                        next_directive = TextDirectiveKind::End;
                    }
                    directive.end.is_empty() && directive.suffix.is_empty()
                },
                TextDirectiveKind::End => {
                    if !directive.suffix.is_empty() {
                        next_directive = TextDirectiveKind::Suffix;
                    }
                    directive.suffix.is_empty()
                },
                TextDirectiveKind::Suffix => { true },
            };
            
            td.status = CheckerStatus::NotFound.into();

            let start_offset = td.next_offset.borrow().get();
            if let Some(status) = self.content.borrow().find(&search_str, start_offset, start_bounded_word, end_bounded_word) {
                match status {
                    CheckerStatus::Found((start, end)) => {
                        match search_kind {
                            TextDirectiveKind::Prefix => {},
                            TextDirectiveKind::Start => {
                                let mut found_content = self.content.borrow().get_content(Some(start..end));

                                // [UGLY]: If prefix is found, and if it part of the starting word, then let us skip the prefix
                                if !directive.prefix.is_empty() && start == start_offset {
                                    found_content = found_content.replace(&directive.prefix, "");
                                }
                                td.update_result_str(&found_content);    
                            },
                            TextDirectiveKind::End => {
                                let found_content = self.content.borrow().get_content(Some(start_offset..end));
                                td.update_result_str(&found_content);
                            },
                            TextDirectiveKind::Suffix => {
                                // Suffix MUST be found on the start_offset (or) in the immediate word next to it
                                // **Note:** start is relative to the start offset and hence shall be 0 or 1
                                // any value greater than 1 implies the directive rule was not satisfied!!!
                                if start - start_offset > 1 {
                                    td.reset();
                                    break 'directive_loop;
                                }
                                
                                // repeat the prefix clean-up logic here too
                                let found_content = td.resultant_str.borrow_mut().replace(&directive.suffix, "");
                                td.clear_result_str();
                                td.update_result_str(&found_content);
                            },
                        }

                        // Let us save the end as the next start offset (for Suffix directives)
                        let mut next_offset = end;
                        if end_bounded_word {
                            next_offset += 1;
                        }
                        td.next_offset = next_offset.into();

                        // We've matched all the text directives...time to exit!
                        if next_directive == search_kind {
                            td.status = CheckerStatus::Completed.into();
                            // info!("found: {}", td.get_result_str());
                            break 'directive_loop;
                        }
                    },
                    CheckerStatus::NotFound => {
                        // If the directive kind is End, we MIGHT find the end in  some other block element's content -
                        // until then, we keep collecting the block element contents
                        if TextDirectiveKind::End == *td.search_kind.borrow() {
                            let end = self.content.borrow().word_count().saturating_sub(1);
                            let range = if end > 0 {
                                Some(start_offset..end)
                            } else {
                                None
                            };

                            let end_content = self.content.borrow().get_content(range);                            
                            td.update_result_str(&end_content);
                        }

                        // reset the search kind, status and offset fields
                        td.status = CheckerStatus::NotFound.into();
                        td.reset();
                        break 'directive_loop;
                    },
                    CheckerStatus::EndOfContent => {
                        td.reset();
                        break 'directive_loop;
                    },
                    CheckerStatus::NotStarted | CheckerStatus::Completed => {}
                }
            } 

            td.search_kind = next_directive.into();
        }
    }
}

#[derive(PartialEq, Clone, Debug, Default)]
enum TextDirectiveKind {
    /// Prefix
    Prefix,
    /// Start
    #[default]
    Start,
    /// End
    End,
    /// Suffix
    Suffix,
}

#[derive(PartialEq, Clone, Debug, Default)]
pub(crate) enum CheckerStatus {
    /// Not started - marks the next text directive kind to be sought
    #[default]
    NotStarted,
    /// Text Directive is found in the content
    /// and return start offset and end index of the search string
    Found((usize, usize)), 
    /// Not Found
    NotFound,
    /// End of content - indicator to start searching from start of the 
    /// next block element
    EndOfContent,
    /// Completed directive checks successfully
    Completed,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TextDirectiveChecker {
    /// Parsed `TextDirective` object
    directive: TextDirective,
    /// Checker Status
    status: RefCell<CheckerStatus>,
    /// Current search string - this will be dynamically updated by the Checker state machine
    search_kind: RefCell<TextDirectiveKind>,
    /// expected next offset for `**search_str**` to be found from
    next_offset: Cell<usize>,
    /// Directive string result
    resultant_str: RefCell<String>,
}

impl TextDirectiveChecker {
    fn new(td: &TextDirective) -> Self {
        let mut directive_kind = TextDirectiveKind::Prefix;
        if td.prefix.is_empty() { 
            directive_kind = TextDirectiveKind::Start;
        };

        Self {
            directive: td.clone(), // RefCell::new(td.clone()),
            status: RefCell::new(CheckerStatus::NotStarted),
            search_kind: RefCell::new(directive_kind.clone()),
            next_offset: Cell::new(0),
            resultant_str: RefCell::new(String::new()),
        }
    }

    fn reset(&self) {
        // reset the search kind, and offset fields
        self.next_offset.borrow().set(0);
        
        // If the next directive is End, we retain the resultant string found so far
        if TextDirectiveKind::End != *self.search_kind.borrow() {
            self.resultant_str.borrow_mut().clear();

            // Restart the search
            *self.search_kind.borrow_mut() = TextDirectiveKind::Start;

            let directive = self.directive.borrow();
            if !directive.prefix.is_empty() {
                *self.search_kind.borrow_mut() = TextDirectiveKind::Prefix;
            } 
        }
    }

    /// Update resultant string content (padding with whitespace, for readability)
    fn update_result_str(&self, content: &str) {
        let mut res_str = self.resultant_str.borrow_mut();

        if !res_str.is_empty() {
            res_str.push(' ');
        }
        res_str.push_str(content);
    }

    fn clear_result_str(&self) {
        self.resultant_str.borrow_mut().clear();
    }

    pub(crate) fn get_result_str(&self) -> String {
        self.resultant_str.borrow().to_string()
    }

    pub(crate) fn get_status(&self) -> CheckerStatus {
        self.status.borrow().clone()
    }

    pub(crate) fn get_text_directive(&self) -> String {
        self.directive/*.borrow()*/.raw_directive.clone()
    }
}

#[cfg(test)]
mod tests {
    const HTML_INPUT: &str = 
    "<html>
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

    use crate::types::{FragmentDirective, TextFragmentStatus};
    use super::*;

    use html5ever::tokenizer::{BufferQueue, Tokenizer, TokenizerOpts};
    use http::StatusCode;
    use log::info;

    use crate::{types::ErrorKind, Status};

    /// Fragment Directive checker method - takes website response text and text directives 
    /// as input and returns Directive check status (as HTTP Status)
    /// 
    /// # Errors
    /// - 
    fn check(buf: &str, text_directives: Vec<TextDirective>)  -> Result<Status, ErrorKind> {
        let fd_checker = FragmentDirectiveTokenizer::new(text_directives);

        let tok = Tokenizer::new(
            fd_checker, 
            TokenizerOpts {
                profile: true,
                ..Default::default()
            },
        );

        let input = BufferQueue::default();
        input.pop_front();
        input.push_back(buf.into());

        let res = tok.feed(&input);
        info!("{res:?}");
        tok.end();

        let mut error = None;

        for td in tok.sink.text_directives.borrow().iter() {
            let status = td.get_status();
            info!("tdirective: {:?}", td.get_text_directive());
            info!("status: {:?}", status);
            info!("result: {:?}", td.get_result_str());

            if status != CheckerStatus::Completed {
                error = Some(ErrorKind::TextFragmentError(TextFragmentStatus::TextDirectiveNotFound(td.get_text_directive())));
            }
        }

        if let Some(error) = error {
            return Err(error);
        }

        Ok(Status::Ok(StatusCode::OK))
    }

    #[test]
    fn test_fragment_directive_checker() {
        const FRAGMENT: &str = ":~:text=par-,agraph,inp,-ut";
        let fd = FragmentDirective::from_fragment_as_str(&FRAGMENT);

        let res = check(&HTML_INPUT, fd.unwrap().text_directives);
        match res {
            Ok(result) => {
                assert_eq!(result, Status::Ok(StatusCode::OK));
            }
            Err(e) => {
                panic!("{}", e);
            },
        };
    }
}
