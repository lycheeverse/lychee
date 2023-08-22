use log::info;
use percent_encoding::percent_decode_str;
use reqwest::Url;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{
    basic_auth::BasicAuthExtractor,
    types::{uri::raw::RawUri, InputContent, InputSource},
    utils::{path, url},
    Base, BasicAuthCredentials, ErrorKind, Request, Result, Uri,
};

const MAX_TRUNCATED_STR_LEN: usize = 100;

/// Extract basic auth credentials for a given URL.
fn credentials(extractor: &Option<BasicAuthExtractor>, uri: &Uri) -> Option<BasicAuthCredentials> {
    extractor.as_ref().and_then(|ext| ext.matches(uri))
}

/// Create requests out of the collected URLs.
/// Only keeps "valid" URLs. This filters out anchors for example.
pub(crate) fn create(
    uris: Vec<RawUri>,
    input_content: &InputContent,
    base: &Option<Base>,
    extractor: &Option<BasicAuthExtractor>,
) -> Result<HashSet<Request>> {
    let base_url = Base::from_source(&input_content.source);

    let requests: Result<Vec<Option<Request>>> = uris
        .into_iter()
        .map(|raw_uri| {
            let is_anchor = raw_uri.is_anchor();
            let text = raw_uri.text.clone();
            let element = raw_uri.element.clone();
            let attribute = raw_uri.attribute.clone();

            // Truncate the source in case it gets too long Ideally we should
            // avoid the initial String allocation for `source` altogether
            let source = match &input_content.source {
                InputSource::String(s) => {
                    InputSource::String(s.chars().take(MAX_TRUNCATED_STR_LEN).collect())
                }
                // Cloning is cheap here
                c => c.clone(),
            };

            if let Ok(uri) = Uri::try_from(raw_uri) {
                let credentials = credentials(extractor, &uri);

                Ok(Some(Request::new(
                    uri,
                    source,
                    element,
                    attribute,
                    credentials,
                )))
            } else if let Some(url) = base.as_ref().and_then(|u| u.join(&text)) {
                let uri = Uri { url };
                let credentials = credentials(extractor, &uri);

                Ok(Some(Request::new(
                    uri,
                    source,
                    element,
                    attribute,
                    credentials,
                )))
            } else if let InputSource::FsPath(root) = &input_content.source {
                let path = if is_anchor {
                    match root.file_name() {
                        Some(file_name) => match file_name.to_str() {
                            Some(valid_str) => valid_str.to_string() + &text,
                            None => return Err(ErrorKind::InvalidFile(root.clone())),
                        },
                        None => return Err(ErrorKind::InvalidFile(root.clone())),
                    }
                } else {
                    text
                };

                if let Some(url) = create_uri_from_path(root, &path, base)? {
                    let uri = Uri { url };
                    let credentials = credentials(extractor, &uri);

                    Ok(Some(Request::new(
                        uri,
                        source,
                        element,
                        attribute,
                        credentials,
                    )))
                } else {
                    // In case we cannot create a URI from a path but we didn't receive an error,
                    // it means that some preconditions were not met, e.g. the `base_url` wasn't set.
                    Ok(None)
                }
            } else if let Some(url) = construct_url(&base_url, &text) {
                if base.is_some() {
                    Ok(None)
                } else {
                    let uri = Uri { url: url? };
                    let credentials = credentials(extractor, &uri);

                    Ok(Some(Request::new(
                        uri,
                        source,
                        element,
                        attribute,
                        credentials,
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

fn construct_url(base: &Option<Url>, text: &str) -> Option<Result<Url>> {
    base.as_ref().map(|base| {
        base.join(text)
            .map_err(|e| ErrorKind::ParseUrl(e, format!("{base}{text}")))
    })
}

fn create_uri_from_path(src: &Path, dst: &str, base: &Option<Base>) -> Result<Option<Url>> {
    let (dst, frag) = url::remove_get_params_and_separate_fragment(dst);
    // Avoid double-encoding already encoded destination paths by removing any
    // potential encoding (e.g. `web%20site` becomes `web site`).
    // That's because Url::from_file_path will encode the full URL in the end.
    // This behavior cannot be configured.
    // See https://github.com/lycheeverse/lychee/pull/262#issuecomment-915245411
    // TODO: This is not a perfect solution.
    // Ideally, only `src` and `base` should be URL encoded (as is done by
    // `from_file_path` at the moment) while `dst` gets left untouched and simply
    // appended to the end.
    let decoded = percent_decode_str(dst).decode_utf8()?;
    let resolved = path::resolve(src, &PathBuf::from(&*decoded), base)?;
    match resolved {
        Some(path) => Url::from_file_path(&path)
            .map(|mut url| {
                url.set_fragment(frag);
                url
            })
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
