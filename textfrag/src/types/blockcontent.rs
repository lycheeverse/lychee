/// HTML Block element content
use std::{
    borrow::{Borrow, BorrowMut},
    cell::{Cell, RefCell},
    ops::Range,
};

use crate::types::status::TextDirectiveStatus;

/// HTML Block element content object
#[derive(Clone, Default)]
pub struct BlockElementContent {
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
    /// Control code - to compute word count (inline)
    new_word: Cell<bool>,
    /// word count
    nwords: Cell<usize>,
    /// [TODO] Indicate if the content is right to left
    _rtl: Cell<bool>,
}

impl BlockElementContent {
    pub const fn new() -> Self {
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

    /// name of the current block element
    pub fn set_name(&self, name: &str) {
        let mut elt = self.element_name.borrow_mut();
        if !elt.is_empty() {
            elt.clear();
        }
        elt.push_str(name);
    }

    /// Block Start line number
    pub fn set_start_line(&self, line_number: u64) {
        self.start_line_number.borrow().set(line_number);
    }

    /// Block ending line number
    pub fn set_end_line(&self, line_number: u64) {
        self.end_line_number.borrow().set(line_number);
    }

    /// update block content
    pub fn set_content(&mut self, c: char) {
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
        // increment nwords count
        if !self.new_word.borrow().get() && is_word {
            nwords += 1;
        }

        let buf = c.escape_default().collect::<String>(); //.replace("\\n", " ");
        self.content.borrow_mut().push_str(&buf);

        self.nwords.borrow_mut().set(nwords);
        self.new_word.borrow_mut().set(is_word);
    }

    pub fn word_count(&self) -> usize {
        self.nwords.borrow().get()
    }

    fn get_content_by_range(&self, start: usize, end: usize) -> String {
        let content = self
            .content
            .borrow()
            .clone()
            .split_whitespace()
            .collect::<Vec<&str>>()[start..=end]
            .join(" ");
        content
    }

    pub fn get_content(&self, range: Option<Range<usize>>) -> String {
        if let Some(r) = range {
            self.get_content_by_range(r.start, r.end)
        } else {
            self.content.borrow().clone()
        }
    }

    pub fn clear(&mut self) {
        self.content.borrow_mut().clear();
        self.new_word.borrow_mut().set(false);
        self.nwords.borrow_mut().set(0);
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible.borrow_mut().set(visible);
    }

    /// Find the `**search_str**` in the block content and, when found, return a pair of
    /// start and end offset on the block content
    /// If content is hidden and/or no match is found, return None
    pub fn find(
        &self,
        search_str: &str,
        start_offset: usize,
        start_bounded_word: bool,
        end_bounded_word: bool,
        allowed_word_distance: i32,
    ) -> Option<TextDirectiveStatus> {
        if !self.visible.borrow().get() {
            return None;
        }

        if self.word_count() < start_offset {
            return Some(TextDirectiveStatus::EndOfContent);
        }

        let c_words = self.get_content(None);
        let c_words = c_words
            .split_whitespace()
            .skip(start_offset)
            .map(str::to_lowercase)
            .collect::<Vec<String>>();
        let mut c_words_iter = c_words.into_iter().enumerate();

        let mut found = false;
        let mut start_index = None;

        let search_str = search_str.escape_default().to_string();
        let swords = search_str
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<String>>();

        for mut i in 0..c_words_iter.len() {
            let mut len = 0;
            for sw in swords.iter() {
                found = false;
                if let Some(cword) = c_words_iter.next() {
                    // if word is found beyond the max_word_distance, we'll break from
                    // the search words loop and restart the search amongst rest of the
                    // block content
                    if start_index.is_none()
                        && allowed_word_distance > -1
                        && i > allowed_word_distance as usize
                    {
                        log::warn!("proximity search failed!");
                        return Some(TextDirectiveStatus::WordDistanceExceeded(i + start_offset));
                    }
                    i += 1;

                    found = start_bounded_word && end_bounded_word && sw.eq(&cword.1)
                        || start_bounded_word && cword.1.starts_with(sw)
                        || end_bounded_word && cword.1.ends_with(sw)
                        || cword.1.contains(sw);

                    if found {
                        len += 1;

                        if start_index.is_none() {
                            start_index = Some(cword.0);
                        }

                        // continue with the remaining search words...
                        continue;
                    }
                }

                if !found {
                    start_index = None;
                    break;
                }
            }

            // We've looped through the search words and found a match for all of those
            if found {
                debug_assert!(
                    len as usize == search_str.split_whitespace().collect::<Vec<&str>>().len()
                );

                let start = start_index.unwrap() + start_offset;
                let end = start + (len as usize) - 1;

                return Some(TextDirectiveStatus::Found((start, end)));
            }
        }

        Some(TextDirectiveStatus::NotFound)
    }
}
