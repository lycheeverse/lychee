use crate::extract::{extract_links, FileType};
use crate::types::Uri;
use anyhow::Result;
use glob::glob_with;
use reqwest::Url;
use shellexpand::tilde;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs::read_to_string;
use tokio::io::{stdin, AsyncReadExt};

const STDIN: &str = "-";

#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum Input {
    RemoteUrl(Url),
    FsGlob { pattern: String, ignore_case: bool },
    FsPath(PathBuf),
    Stdin,
    String(String),
}

#[derive(Debug)]
pub(crate) struct InputContent {
    pub input: Input,
    pub file_type: FileType,
    pub content: String,
}

impl InputContent {
    pub fn from_string(s: &str, file_type: FileType) -> Self {
        // TODO: consider using Cow (to avoid one .clone() for String types)
        Self {
            input: Input::String(s.to_owned()),
            file_type,
            content: s.to_owned(),
        }
    }
}

impl Input {
    pub(crate) fn new(value: &str, glob_ignore_case: bool) -> Self {
        if value == STDIN {
            Self::Stdin
        } else {
            match Url::parse(&value) {
                Ok(url) => Self::RemoteUrl(url),
                // we assume that it's cheaper to just do the globbing, without
                // checking if the `value` actually is a glob pattern
                Err(_) => Self::FsGlob {
                    pattern: value.to_owned(),
                    ignore_case: glob_ignore_case,
                },
            }
        }
    }

    pub async fn get_contents(
        &self,
        file_type_hint: Option<FileType>,
    ) -> Result<Vec<InputContent>> {
        use Input::*;

        let contents = match self {
            RemoteUrl(url) => vec![Self::url_contents(url).await?],
            FsGlob {
                pattern,
                ignore_case,
            } => Self::glob_contents(pattern, *ignore_case).await?,
            FsPath(path) => vec![Self::path_content(&path).await?],
            Stdin => vec![Self::stdin_content(file_type_hint).await?],
            String(s) => vec![Self::string_content(s, file_type_hint)?],
        };

        Ok(contents)
    }

    async fn url_contents(url: &Url) -> Result<InputContent> {
        let res = reqwest::get(url.clone()).await?;
        let content = res.text().await?;
        let input_content = InputContent {
            input: Input::RemoteUrl(url.clone()),
            file_type: FileType::from(url.as_str()),
            content,
        };

        Ok(input_content)
    }

    async fn glob_contents(path_glob: &str, ignore_case: bool) -> Result<Vec<InputContent>> {
        let mut contents = vec![];
        let glob_expanded = tilde(&path_glob);
        let mut match_opts = glob::MatchOptions::new();

        match_opts.case_sensitive = !ignore_case;

        println!("GLOB {:?} ignore case {:?}", path_glob, ignore_case);

        for entry in glob_with(&glob_expanded, match_opts)? {
            match entry {
                Ok(path) => {
                    let content = Self::path_content(&path).await?;
                    contents.push(content);
                }
                Err(e) => println!("{:?}", e),
            }
        }

        Ok(contents)
    }

    async fn path_content<P: Into<PathBuf> + AsRef<Path>>(path: P) -> Result<InputContent> {
        let input_content = InputContent {
            file_type: FileType::from(path.as_ref()),
            content: read_to_string(&path).await?,
            input: Input::FsPath(path.into()),
        };

        Ok(input_content)
    }

    async fn stdin_content(file_type_hint: Option<FileType>) -> Result<InputContent> {
        let mut content = String::new();
        let mut stdin = stdin();
        stdin.read_to_string(&mut content).await?;

        let input_content = InputContent {
            input: Input::Stdin,
            file_type: file_type_hint.unwrap_or_default(),
            content,
        };

        Ok(input_content)
    }

    fn string_content(s: &str, file_type_hint: Option<FileType>) -> Result<InputContent> {
        Ok(InputContent::from_string(
            s,
            file_type_hint.unwrap_or_default(),
        ))
    }
}

impl ToString for Input {
    fn to_string(&self) -> String {
        match self {
            Self::RemoteUrl(url) => url.to_string(),
            Self::FsGlob { pattern, .. } => pattern.clone(),
            Self::FsPath(p) => p.to_str().unwrap_or_default().to_owned(),
            Self::Stdin => STDIN.to_owned(),
            Self::String(s) => s.clone(),
        }
    }
}

/// Fetch all unique links from a slice of inputs
/// All relative URLs get prefixed with `base_url` if given.
pub(crate) async fn collect_links(
    inputs: &[Input],
    base_url: Option<String>,
) -> Result<HashSet<Uri>> {
    let base_url = match base_url {
        Some(url) => Some(Url::parse(&url)?),
        _ => None,
    };

    let mut links = HashSet::new();

    for input in inputs {
        let input_contents = input.get_contents(None).await?;

        for input_content in input_contents {
            links.extend(extract_links(&input_content, base_url.clone()));
        }
    }
    Ok(links)
}
