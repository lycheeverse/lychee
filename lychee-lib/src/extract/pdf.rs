use crate::{types::uri::raw::RawUri, InputContent, Result};
use mupdf::Document;

/// Extract unparsed URL strings from a PDF document.
pub(crate) fn extract_pdf(input: &InputContent, _include_verbatim: bool) -> Result<Vec<RawUri>> {
    let document = Document::from_bytes(&input.content, &input.file_type.to_string())?;

    let mut urls = Vec::new();
    for page in &document {
        let page = page?;
        for link in page.links()? {
            // Element and attribute are currently not provided
            urls.push(RawUri {
                text: link.uri.to_string(),
                element: None,
                attribute: None,
            });
        }
    }

    Ok(urls)
}
