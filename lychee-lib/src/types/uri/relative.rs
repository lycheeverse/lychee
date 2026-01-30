use either::{Either, Left, Right};
use url::ParseError;

use crate::ErrorKind;
use crate::Uri;

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum RelativeUri<'a> {
    RootRel(&'a str),
    SchemeRel(&'a str),
    LocalRel(&'a str),
}

pub(crate) use RelativeUri::{LocalRel, RootRel, SchemeRel};

impl RelativeUri<'_> {
    pub(crate) fn link_text(&self) -> &str {
        match self {
            RootRel(x) | SchemeRel(x) | LocalRel(x) => x,
        }
    }
}

fn is_root_relative_link(text: &str) -> bool {
    !is_scheme_relative_link(text) && text.trim_ascii_start().starts_with('/')
}

fn is_scheme_relative_link(text: &str) -> bool {
    text.trim_ascii_start().starts_with("//")
}

pub(crate) fn parse_url_or_relative(text: &str) -> Result<Either<Uri, RelativeUri<'_>>, ErrorKind> {
    let text = text.trim_ascii_start();

    match Uri::try_from(text) {
        Ok(uri) => Ok(Left(uri)),

        Err(ErrorKind::ParseUrl(ParseError::RelativeUrlWithoutBase, _)) => {
            if is_scheme_relative_link(text) {
                Ok(Right(SchemeRel(text)))
            } else if is_root_relative_link(text) {
                Ok(Right(RootRel(text)))
            } else {
                Ok(Right(LocalRel(text)))
            }
        }
        Err(e) => Err(e),
    }
}
