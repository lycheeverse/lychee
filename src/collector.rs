use crate::extract::{extract_links, FileType};
use crate::types::Uri;
use anyhow::Result;
use glob::glob;
use reqwest::Url;
use std::path::Path;
use std::{collections::HashSet, fs};

/// Detect if the given path points to a Markdown, HTML, or plaintext file.
fn resolve_file_type_by_path<P: AsRef<Path>>(p: P) -> FileType {
    let path = p.as_ref();
    match path.extension() {
        Some(ext) => match ext.to_str().unwrap() {
            "md" => FileType::Markdown,
            "html" | "htm" => FileType::HTML,
            _ => FileType::Plaintext,
        },
        None => FileType::Plaintext,
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
