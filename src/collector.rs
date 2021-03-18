use crate::{
    extract::{extract_links, FileType},
    Request,
};
use anyhow::{anyhow, Context, Result};
use glob::glob_with;
use reqwest::Url;
use serde::Serialize;
use shellexpand::tilde;
use std::path::Path;
use std::path::PathBuf;
use std::{collections::HashSet, fmt::Display};
use tokio::fs::read_to_string;
use tokio::io::{stdin, AsyncReadExt};

const STDIN: &str = "-";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Input {
    RemoteUrl(Url),
    FsGlob { pattern: String, ignore_case: bool },
    FsPath(PathBuf),
    Stdin,
    String(String),
}

impl Serialize for Input {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Input::RemoteUrl(url) => {
                write!(f, "{}", url)
            }
            Input::FsGlob {
                pattern,
                ignore_case: _,
            } => {
                write!(f, "{}", pattern)
            }
            Input::FsPath(path) => {
                write!(f, "{}", path.to_str().unwrap_or_default())
            }
            Input::Stdin => {
                write!(f, "stdin")
            }
            Input::String(_) => {
                write!(f, "raw input string")
            }
        }
    }
}

#[derive(Debug)]
pub struct InputContent {
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
    pub fn new(value: &str, glob_ignore_case: bool) -> Self {
        if value == STDIN {
            Self::Stdin
        } else {
            match Url::parse(&value) {
                Ok(url) => Self::RemoteUrl(url),
                Err(_) => {
                    // this seems to be the only way to determine if this is a glob pattern
                    let is_glob = glob::Pattern::escape(value) != value;

                    if is_glob {
                        Self::FsGlob {
                            pattern: value.to_owned(),
                            ignore_case: glob_ignore_case,
                        }
                    } else {
                        Self::FsPath(value.into())
                    }
                }
            }
        }
    }

    pub async fn get_contents(
        &self,
        file_type_hint: Option<FileType>,
        skip_missing: bool,
    ) -> Result<Vec<InputContent>> {
        use Input::*;

        match self {
            // TODO: should skip_missing also affect URLs?
            RemoteUrl(url) => Ok(vec![Self::url_contents(url).await?]),
            FsGlob {
                pattern,
                ignore_case,
            } => Ok(Self::glob_contents(pattern, *ignore_case).await?),
            FsPath(path) => {
                let content = Self::path_content(&path).await.with_context(|| {
                    format!(
                        "Failed to read file: `{}`",
                        path.to_str().unwrap_or("<MALFORMED PATH>")
                    )
                });
                match content {
                    Ok(input_content) => Ok(vec![input_content]),
                    Err(_) if skip_missing => Ok(vec![]),
                    Err(arg) => Err(anyhow!(arg)),
                }
            }
            Stdin => Ok(vec![Self::stdin_content(file_type_hint).await?]),
            String(s) => Ok(vec![Self::string_content(s, file_type_hint)]),
        }
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

    fn string_content(s: &str, file_type_hint: Option<FileType>) -> InputContent {
        InputContent::from_string(s, file_type_hint.unwrap_or_default())
    }
}

/// Fetch all unique links from a slice of inputs
/// All relative URLs get prefixed with `base_url` if given.
pub async fn collect_links(
    inputs: Vec<Input>,
    base_url: Option<String>,
    skip_missing_inputs: bool,
    max_concurrency: usize,
) -> Result<tokio::sync::mpsc::Receiver<Request>> {
    let base_url = match base_url {
        Some(url) => Some(Url::parse(&url)?),
        _ => None,
    };

    let (contents_tx, mut contents_rx) = tokio::sync::mpsc::channel(max_concurrency);

    // extract input contents
    for input in inputs {
        let sender = contents_tx.clone();

        tokio::spawn(async move {
            let contents = input.get_contents(None, skip_missing_inputs).await;
            sender.send(contents).await
        });
    }

    // receiver will get None once all tasks are done
    drop(contents_tx);

    // extract links from input contents
    let mut extract_links_handles = vec![];

    while let Some(result) = contents_rx.recv().await {
        for input_content in result? {
            let base_url = base_url.clone();
            let handle =
                tokio::task::spawn_blocking(move || extract_links(&input_content, base_url));
            extract_links_handles.push(handle);
        }
    }

    let (links_tx, links_rx) = tokio::sync::mpsc::channel(max_concurrency);
    tokio::spawn(async move {
        let mut collected_links = HashSet::new();

        for handle in extract_links_handles {
            // Unwrap should be fine because joining fails:
            // * if the Task was dropped (which we don't do)
            // * if the Task panicked. Propagating panics is correct here.
            let requests = handle
                .await
                .expect("Awaiting termination of link handle failed");
            for request in requests {
                if !collected_links.contains(&request) {
                    collected_links.insert(request.clone());
                    // Unwrap should be fine because sending fails
                    // if the receiver was closed - in which case we can't continue anyway
                    links_tx
                        .send(request)
                        .await
                        .expect("Extractor could not send link to channel");
                }
            }
        }
    });

    Ok(links_rx)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        test_utils::{get_mock_server_with_content, website},
        Uri,
    };
    use std::fs::File;
    use std::io::Write;
    use std::str::FromStr;

    const TEST_STRING: &str = "http://test-string.com";
    const TEST_URL: &str = "https://test-url.org";
    const TEST_FILE: &str = "https://test-file.io";
    const TEST_GLOB_1: &str = "https://test-glob-1.io";
    const TEST_GLOB_2_MAIL: &str = "test@glob-2.io";

    #[tokio::test]
    async fn test_collect_links() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join("f");
        let file_glob_1_path = dir.path().join("glob-1");
        let file_glob_2_path = dir.path().join("glob-2");

        let mut file = File::create(&file_path)?;
        let mut file_glob_1 = File::create(file_glob_1_path)?;
        let mut file_glob_2 = File::create(file_glob_2_path)?;

        writeln!(file, "{}", TEST_FILE)?;
        writeln!(file_glob_1, "{}", TEST_GLOB_1)?;
        writeln!(file_glob_2, "{}", TEST_GLOB_2_MAIL)?;

        let mock_server = get_mock_server_with_content(http::StatusCode::OK, Some(TEST_URL)).await;

        let inputs = vec![
            Input::String(TEST_STRING.to_string()),
            Input::RemoteUrl(Url::from_str(&mock_server.uri())?),
            Input::FsPath(file_path),
            Input::FsGlob {
                pattern: dir.path().join("glob*").to_str().unwrap().to_string(),
                ignore_case: true,
            },
        ];

        let mut responses = collect_links(inputs, None, false, 8).await?;
        let mut links = HashSet::new();
        while let Some(request) = responses.recv().await {
            links.insert(request.uri);
        }

        let mut expected_links: HashSet<Uri> = HashSet::new();
        expected_links.insert(website(TEST_STRING));
        expected_links.insert(website(TEST_URL));
        expected_links.insert(website(TEST_FILE));
        expected_links.insert(website(TEST_GLOB_1));
        expected_links.insert(Uri::Mail(TEST_GLOB_2_MAIL.to_string()));

        assert_eq!(links, expected_links);

        Ok(())
    }
}
