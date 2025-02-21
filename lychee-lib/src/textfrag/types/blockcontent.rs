//! HTML Block element content
//!
//! This module defines the `BlockElementContent` struct, used to store and access the HTML
//! block element content. The implementation includes getters/setters of the struct's members,
//! methods to manage visibility, and find method to search the content.
//!
//! It stores the element name, start and end line numbers, content, visibility flag, and word
//! count. It provides methods to update and retrieve this information, as well as to search for
//! specific text within the content.
//!
//! This struct is constructed and consumed by the html5ever tokenizer.
//!
//! # Example
//!
//! ```rust
//! use lychee_lib::textfrag::types::blockcontent::BlockElementContent;
//! use lychee_lib::textfrag::types::status::TextDirectiveStatus;
//!
//! let mut block_content = BlockElementContent::new();
//! block_content.set_name("div");
//! block_content.set_start_line(1);
//! let content = "testing the find method";
//! for c in content.chars() {
//!     block_content.set_content(c);
//! }
//! block_content.set_end_line(2);
//!
//! let content = block_content.get_content(None);
//! println!("Block content: {}", content);
//!
//! let status = block_content.find("the", 0, true, false, -1);
//! assert_eq!(status, Some(TextDirectiveStatus::Found((1, 1))));
//! ```
use std::{
    borrow::{Borrow, BorrowMut},
    cell::{Cell, RefCell},
    ops::Range,
};

use crate::textfrag::types::status::TextDirectiveStatus;

/// HTML Block element content object
#[derive(Clone, Debug, Default)]
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
    #[must_use]
    /// Construct block element content object
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

    #[allow(dead_code)]
    /// Returns the element name
    fn get_name(&self) -> String {
        self.element_name.borrow().to_string()
    }

    /// Block Start line number
    pub fn set_start_line(&self, line_number: u64) {
        self.start_line_number.borrow().set(line_number);
    }

    #[allow(dead_code)]
    /// Getter for block content's start line number
    pub fn get_start_line(&self) -> u64 {
        self.start_line_number.borrow().get()
    }

    /// Block ending line number
    pub fn set_end_line(&self, line_number: u64) {
        self.end_line_number.borrow().set(line_number);
    }

    #[allow(dead_code)]
    /// Getter for block content's end line number
    pub fn get_end_line(&self) -> u64 {
        self.end_line_number.borrow().get()
    }

    /// updates the block content - the method also identifies the words while
    /// processing and maintains word count for the block element
    /// content
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

        let buf = c.escape_default().collect::<String>();
        self.content.borrow_mut().push_str(&buf);

        self.nwords.borrow_mut().set(nwords);
        self.new_word.borrow_mut().set(is_word);
    }

    /// Returns the word count of the block content
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

    /// Returns the block content over a given word offset range
    /// if range is None, the entire block content is returned
    pub fn get_content(&self, range: Option<Range<usize>>) -> String {
        if let Some(r) = range {
            self.get_content_by_range(r.start, r.end)
        } else {
            self.content.borrow().clone()
        }
    }

    /// Clears the block content and resets the word count to 0
    pub fn clear(&mut self) {
        self.content.borrow_mut().clear();
        self.new_word.borrow_mut().set(false);
        self.nwords.borrow_mut().set(0);
    }

    /// Marks the block content as visible or not
    pub fn set_visible(&mut self, visible: bool) {
        self.visible.borrow_mut().set(visible);
    }

    /// Find the `**search_str**` in the block content and, when found, return a pair of
    /// start and end offset on the block content
    ///
    /// When match is found, returns `TextDirectiveStatus::Found` with the start and end offset
    /// of the search string in the block content.
    ///
    /// If content is hidden, return None
    /// If no match is found, `TextDirectiveStatus::NotFound` is returned
    /// If start offset is beyond the `word_count`, returns `TextDirectiveStatus::EndOfContent`
    /// If the match is found beyond the `allowed_word_distance`, returns `TextDirectiveStatus::WordDistanceExceeded`
    #[allow(clippy::cast_sign_loss)]
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
            for sw in &swords {
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

                let mut start = start_offset;
                if let Some(start_index) = start_index {
                    start = start_index + start_offset;
                }
                let end = start + (len as usize) - 1;

                return Some(TextDirectiveStatus::Found((start, end)));
            }
        }

        Some(TextDirectiveStatus::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_set_name() {
        let block_content = BlockElementContent::new();
        block_content.set_name("div");
        assert_eq!(block_content.get_name().as_str(), "div");
    }

    #[test]
    fn test_set_start_line() {
        let block_content = BlockElementContent::new();
        block_content.set_start_line(1);
        assert_eq!(block_content.get_start_line(), 1);
    }

    #[test]
    fn test_set_end_line() {
        let block_content = BlockElementContent::new();
        block_content.set_end_line(2);
        assert_eq!(block_content.get_end_line(), 2);
    }

    #[test]
    fn test_set_get_content() {
        let mut block_content = BlockElementContent::new();
        let content = "sample content for test!";
        for c in content.chars() {
            block_content.set_content(c);
        }

        assert_eq!(block_content.get_content(None), content);
        assert_eq!(block_content.get_content(Some(1..3)), "content for test!");
    }

    #[test]
    fn test_word_count() {
        let mut block_content = BlockElementContent::new();
        let content = "sample   content for \
            test!";
        for c in content.chars() {
            block_content.set_content(c);
        }

        assert_eq!(
            block_content.word_count(),
            content.split_whitespace().count()
        );
    }

    #[test]
    fn test_clear() {
        let mut block_content = BlockElementContent::new();
        block_content.set_content('H');
        block_content.clear();
        assert_eq!(block_content.get_content(None), "");
    }

    #[test]
    fn test_find() {
        let mut block_content = BlockElementContent::new();
        let content = "testing the find method";
        for c in content.chars() {
            block_content.set_content(c);
        }

        assert_eq!(block_content.get_content(Some(0..1)), "testing the");

        let status = block_content.find("testing", 0, true, false, -1);
        assert_eq!(status, Some(TextDirectiveStatus::Found((0, 0))));

        let status = block_content.find("testing the", 0, true, false, 1);
        assert_eq!(status, Some(TextDirectiveStatus::Found((0, 1))));

        let status = block_content.find("find", 0, true, true, 1);
        assert_eq!(status, Some(TextDirectiveStatus::WordDistanceExceeded(2)));

        let status = block_content.find("mthod", 0, true, true, -1);
        assert_eq!(status, Some(TextDirectiveStatus::NotFound));

        let status = block_content.find("the", 5, true, true, -1);
        assert_eq!(status, Some(TextDirectiveStatus::EndOfContent));

        let status = block_content.find("the", 0, true, true, -1);
        assert_eq!(status, Some(TextDirectiveStatus::Found((1, 1))));
    }
}
