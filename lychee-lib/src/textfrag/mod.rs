/// Text fragment tokenizer and directive extraction module
pub mod extract;
/// Defines text fragment module types
pub mod types;

pub use types::*;

use ::url::Url;

/// Public method to checek the text fragments
pub(crate) fn check_text_fragments(
    site_data: &str,
    url: &Url,
) -> Result<FragmentDirectiveStatus, FragmentDirectiveError> {
    if let Some(fd) = url.fragment_directive() {
        // log::debug!("directives: {:?}", fd.text_directives());
        return fd.check(site_data);
    }

    Err(FragmentDirectiveError::DirectiveProcessingError)
}
