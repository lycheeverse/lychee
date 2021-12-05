use html5ever::{
    buffer_queue::BufferQueue,
    tendril::StrTendril,
    tokenizer::{Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};

use super::plaintext::extract_plaintext;
use crate::{types::raw_uri::RawUri, Result};

#[derive(Clone)]
struct LinkExtractor {
    links: Vec<RawUri>,
}

impl TokenSink for LinkExtractor {
    type Handle = ();

    fn process_token(&mut self, token: Token, line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(raw) => self.links.append(&mut extract_plaintext(&raw)),
            Token::CommentToken(_raw) => (),
            Token::TagToken(_tag) => {
                //println!("TOKEN TAG {:?}", tag); // important
                //todo: img, script

                // This is not proper HTML serialization, of course.
                // match tag.kind {
                //     StartTag => print!("TAG  : <\x1b[32m{}\x1b[0m", tag.name),
                //     EndTag => print!("TAG  : <\x1b[31m/{}\x1b[0m", tag.name),
                // }
                // for attr in tag.attrs.iter() {
                //     print!(
                //         " \x1b[36m{}\x1b[0m='\x1b[34m{}\x1b[0m'",
                //         attr.name.local, attr.value
                //     );
                // }
                ()
            }
            Token::ParseError(err) => {
                println!("ERROR: {}", err);
            }
            Token::NullCharacterToken => (), // println!("NULL CHAR TOKEN"),
            Token::DoctypeToken(_doctype) => (), // println!("DOCTYPTE TOKEN: {:?}", doctype),
            Token::EOFToken => (),           // println!("EOF TOKEN"),
        }
        TokenSinkResult::Continue
    }
}

// Recursively walk links in a HTML document, aggregating URL strings in `urls`.
// fn walk_html_links(node: &Handle) -> Vec<StrTendril> {
//     let mut all_urls = Vec::new();
//     match node.data {
//         NodeData::Text { ref contents } => {
//             all_urls.append(&mut extract_plaintext(&contents.borrow()));
//         }
//         NodeData::Comment { ref contents } => {
//             all_urls.append(&mut extract_plaintext(contents));
//         }
//         NodeData::Element {
//             ref name,
//             ref attrs,
//             ..
//         } => {
//             for attr in attrs.borrow().iter() {
//                 let urls = url::extract_links_from_elem_attr(
//                     attr.name.local.as_ref(),
//                     name.local.as_ref(),
//                     attr.value.as_ref(),
//                 );

//                 if urls.is_empty() {
//                     extract_plaintext(&attr.value);
//                 } else {
//                     all_urls.extend(urls.into_iter().map(StrTendril::from).collect::<Vec<_>>());
//                 }
//             }
//         }
//         _ => {}
//     }

//     // recursively traverse the document's nodes -- this doesn't need any extra
//     // exit conditions, because the document is a tree
//     for child in node.children.borrow().iter() {
//         let urls = walk_html_links(child);
//         all_urls.extend(urls);
//     }

//     all_urls
// }

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str) -> Result<Vec<RawUri>> {
    let mut tokenizer = Tokenizer::new(
        LinkExtractor { links: Vec::new() },
        TokenizerOpts::default(),
    );

    let mut input = BufferQueue::new();
    input.push_back(StrTendril::from(buf));

    let _handle = tokenizer.feed(&mut input);
    tokenizer.end();

    Ok(tokenizer.sink.links)
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_extract_link_at_end_of_line() {
//         let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
//         let link = input.trim_end();

//         let urls = extract_html(input).unwrap();
//         assert_eq!(vec![StrTendril::from(link)], urls);
//     }
// }
