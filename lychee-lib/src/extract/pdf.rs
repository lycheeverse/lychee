use crate::{types::uri::raw::RawUri, Result};
use mupdf::Document;

/// Constant passed to `mupdf::Document::open_file` to determine the type of
/// file to open.
const MUPDF_MAGIC_DOCUMENT_TYPE: &str = "pdf";

/// Extract unparsed URL strings from a PDF document.
pub(crate) fn extract_pdf(input: &[u8], _include_verbatim: bool) -> Result<Vec<RawUri>> {
    let document = Document::from_bytes(input, MUPDF_MAGIC_DOCUMENT_TYPE)?;

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
