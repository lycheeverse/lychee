use crate::extract::{extract_links, FileType};
use crate::types::Uri;
use anyhow::Result;
use glob::glob;
use reqwest::Url;
use std::{collections::HashSet, fs};
use std::{ffi::OsStr, path::Path};

/// Detect if the given path points to a Markdown, HTML, or plaintext file.
fn resolve_file_type_by_path<P: AsRef<Path>>(path: P) -> FileType {
    match path.as_ref().extension().and_then(OsStr::to_str) {
        Some("md") => FileType::Markdown,
        Some("html") => FileType::HTML,
        _ => FileType::Plaintext,
    }
}

/// Fetch all unique links from a vector of inputs
/// All relative URLs get prefixed with `base_url` if given.
pub(crate) async fn collect_links(
    inputs: Vec<String>,
    base_url: Option<String>,
) -> Result<HashSet<Uri>> {
    let base_url = match base_url {
        Some(url) => Some(Url::parse(&url)?),
        _ => None,
    };

    let mut links = HashSet::new();

    for input in inputs {
        match Url::parse(&input) {
            Ok(url) => {
                let path = String::from(url.path());
                let res = reqwest::get(url).await?;
                let content = res.text().await?;

                links.extend(extract_links(
                    resolve_file_type_by_path(path),
                    &content,
                    base_url.clone(),
                ));
            }
            Err(_) => {
                // Assume we got a single file or a glob on our hands
                for entry in glob(&input)? {
                    match entry {
                        Ok(path) => {
                            let content = fs::read_to_string(&path)?;
                            links.extend(extract_links(
                                resolve_file_type_by_path(&path),
                                &content,
                                base_url.clone(),
                            ));
                        }
                        Err(e) => println!("Error handling file pattern {}: {:?}", input, e),
                    }
                }
            }
        };
    }
    Ok(links)
}
