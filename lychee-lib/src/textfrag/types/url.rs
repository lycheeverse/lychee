use url::Url;

use crate::textfrag::types::{FragmentDirective, FRAGMENT_DIRECTIVE_DELIMITER};

/// Fragment Directive extension trait
/// We will use the extension trait pattern to extend [`url::Url`] to support the text fragment feature  
pub trait UrlExt {
    /// Checks if the url has a fragment and the fragment directive delimiter is present  
    fn has_fragment_directive(&self) -> bool;

    /// Constructs `FragmentDirective`, if the URL contains a fragment and has fragment directive delimiter
    fn fragment_directive(&self) -> Option<FragmentDirective>;
}

impl UrlExt for Url {
    /// Checks whether the URL has fragment directive or not  
    ///
    /// **Note:** Fragment Directive is possible only for the URL that has a fragment
    fn has_fragment_directive(&self) -> bool {
        if let Some(fragment) = self.fragment() {
            return fragment.contains(FRAGMENT_DIRECTIVE_DELIMITER);
        }

        false
    }

    /// Return this URL's fragment directive, if any
    ///
    /// **Note:** A fragment directive is part of the URL's fragment following the `:~:` delimiter
    fn fragment_directive(&self) -> Option<FragmentDirective> {
        if self.has_fragment_directive() {
            FragmentDirective::from_url(self)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_fs_tree {
    use crate::textfrag::types::TextDirectiveKind;

    use super::*;
    use url::Url;

    #[test]
    fn test_fragment_directive_through_url() {
        let url = Url::parse(
            "https://example.com#:~:text=prefix-,start,end,-suffix&text=unknown_directive",
        );
        assert!(url.is_ok());
        assert!(url.clone().unwrap().has_fragment_directive());

        let fd = url.unwrap().fragment_directive();
        assert!(
            fd.is_some()
                && fd.clone().unwrap().text_directives().len() == 2
                && fd.clone().unwrap().text_directives()[0]
                    .prefix()
                    .eq("prefix")
                && fd.clone().unwrap().text_directives()[0].search_kind()
                    == TextDirectiveKind::Prefix
        );
    }

    #[test]
    fn test_fragment_directive_error() {
        // without fragment directive delimiter
        let url =
            Url::parse("https://example.com#text=prefix-,start,end,-suffix&text=unknown_directive");
        assert!(url.is_ok() && !url.clone().unwrap().has_fragment_directive());

        // malformed fragment directive delimiter
        let url = Url::parse(
            "https://example.com#:~text=prefix-,start,end,-suffix&text=unknown_directive",
        );
        assert!(url.is_ok() && !url.clone().unwrap().has_fragment_directive());
    }
}
