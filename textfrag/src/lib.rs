mod extract;
mod types;
mod utils;

pub use types::*;
use url::Url;

pub fn check_text_fragments(
    site_data: &str,
    url: &Url,
) -> Result<FragmentDirectiveStatus, FragmentDirectiveError> {
    log::info!("checking fragment directive...");
    if let Some(fd) = url.fragment_directive() {
        log::info!("directives: {:?}", fd.text_directives());
        return fd.check(site_data);
    }

    Err(FragmentDirectiveError::Error(
        TextFragmentError::NotTextDirective,
    ))
}
