use html5ever::tendril::{fmt::UTF8, SendTendril, StrTendril};
use log::info;
use percent_encoding::percent_decode_str;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use reqwest::Url;
use std::{
    collections::HashSet,
    convert::TryFrom,
    iter::FromIterator,
    path::{Path, PathBuf},
};

use crate::{
    helpers::{path, url},
    types::{raw_uri::RawUri, InputContent},
    Base, ErrorKind, Input, Request, Result, Uri,
};

/// Create requests out of the collected URLs.
/// Only keeps "valid" URLs. This filters out anchors for example.
pub(crate) fn create_requests(
    uris: Vec<RawUri>,
    input_content: &InputContent,
    base: &Option<Base>,
) -> Result<HashSet<Request>> {
    let base_input = match &input_content.input {
        Input::RemoteUrl(url) => Some(Url::parse(&format!(
            "{}://{}",
            url.scheme(),
            url.host_str().ok_or(ErrorKind::InvalidUrlHost)?
        ))?),
        _ => None,
        // other inputs do not have a URL to extract a base
    };

    let requests: Result<Vec<Option<Request>>> = uris
        .into_par_iter()
        .map(|raw_uri| {
            let is_anchor = raw_uri.is_anchor();
            let text = StrTendril::from(raw_uri.text);
            if let Ok(uri) = Uri::try_from(text.as_ref()) {
                Ok(Some(Request::new(
                    uri,
                    input_content.input.clone(),
                    raw_uri.kind,
                )))
            } else if let Some(url) = base.as_ref().and_then(|u| u.join(&text)) {
                Ok(Some(Request::new(
                    Uri { url },
                    input_content.input.clone(),
                    raw_uri.kind,
                )))
            } else if let Input::FsPath(root) = &input_content.input {
                if is_anchor {
                    // Silently ignore anchor links for now
                    Ok(None)
                } else {
                    if let Some(url) = create_uri_from_path(root, &text, &base)? {
                        Ok(Some(Request::new(
                            Uri { url },
                            input_content.input.clone(),
                            raw_uri.kind,
                        )))
                    } else {
                        // In case we cannot create a URI from a path but we didn't receive an error,
                        // it means that some preconditions were not met, e.g. the `base_url` wasn't set.
                        Ok(None)
                    }
                }
            } else if let Some(url) = base_input.as_ref().map(|u| u.join(&text)) {
                if base.is_some() {
                    Ok(None)
                } else {
                    Ok(Some(Request::new(
                        Uri { url: url? },
                        input_content.input.clone(),
                        raw_uri.kind,
                    )))
                }
            } else {
                info!("Handling of `{}` not implemented yet", text);
                Ok(None)
            }
        })
        .collect();
    let requests: Vec<Request> = requests?.into_iter().flatten().collect();
    Ok(HashSet::from_iter(requests))
}

fn create_uri_from_path(src: &Path, dst: &str, base: &Option<Base>) -> Result<Option<Url>> {
    let dst = url::remove_get_params_and_fragment(dst);
    // Avoid double-encoding already encoded destination paths by removing any
    // potential encoding (e.g. `web%20site` becomes `web site`).
    // That's because Url::from_file_path will encode the full URL in the end.
    // This behavior cannot be configured.
    // See https://github.com/lycheeverse/lychee/pull/262#issuecomment-915245411
    // TODO: This is not a perfect solution.
    // Ideally, only `src` and `base` should be URL encoded (as is done by
    // `from_file_path` at the moment) while `dst` is left untouched and simply
    // appended to the end.
    let decoded = percent_decode_str(dst).decode_utf8()?;
    let resolved = path::resolve(src, &PathBuf::from(&*decoded), &base)?;
    match resolved {
        Some(path) => Url::from_file_path(&path)
            .map(Some)
            .map_err(|_e| ErrorKind::InvalidUrlFromPath(path)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_uri_from_path() {
        let result =
            create_uri_from_path(&PathBuf::from("/README.md"), "test+encoding", &None).unwrap();
        assert_eq!(result.unwrap().as_str(), "file:///test+encoding");
    }
}
