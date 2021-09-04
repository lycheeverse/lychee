use std::{
    collections::HashSet,
    fmt::Display,
    path::{Path, PathBuf},
};

use glob::glob_with;
use reqwest::Url;
use serde::Serialize;
use shellexpand::tilde;
use tokio::{
    fs::read_to_string,
    io::{stdin, AsyncReadExt},
};

use crate::{
    extract::{extract_links, FileType},
    uri::Uri,
    Request, Result,
};

const STDIN: &str = "-";
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
/// An exhaustive list of input sources, which lychee accepts
pub enum Input {
    /// URL (of HTTP/HTTPS scheme).
    RemoteUrl(Box<Url>),
    /// Unix shell-style glob pattern.
    FsGlob {
        /// The glob pattern matching all input files
        pattern: String,
        /// Don't be case sensitive when matching files against a glob
        ignore_case: bool,
    },
    /// File path.
    FsPath(PathBuf),
    /// Standard Input.
    Stdin,
    /// Raw string input.
    String(String),
}

impl Serialize for Input {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Input::RemoteUrl(url) => url.as_str(),
            Input::FsGlob { pattern, .. } => pattern,
            Input::FsPath(path) => path.to_str().unwrap_or_default(),
            Input::Stdin => "stdin",
            Input::String(_) => "raw input string",
        })
    }
}

#[derive(Debug)]
/// Encapsulates the content for a given input
pub struct InputContent {
    /// Input source
    pub input: Input,
    /// File type of given input
    pub file_type: FileType,
    /// Raw UTF-8 string content
    pub content: String,
}

impl InputContent {
    #[must_use]
    /// Create an instance of `InputContent` from an input string
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
    #[must_use]
    /// Construct a new `Input` source. In case the input is a `glob` pattern,
    /// `glob_ignore_case` decides whether matching files against the `glob` is
    /// case-insensitive or not
    pub fn new(value: &str, glob_ignore_case: bool) -> Self {
        if value == STDIN {
            Self::Stdin
        } else if let Ok(url) = Url::parse(value) {
            Self::RemoteUrl(Box::new(url))
        } else {
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

    #[allow(clippy::missing_panics_doc)]
    /// Retrieve the contents from the input
    ///
    /// # Errors
    ///
    /// Returns an error if the contents can not be retrieved
    /// because of an underlying I/O error (e.g. an error while making a
    /// network request or retrieving the contents from the file system)
    pub async fn get_contents(
        &self,
        file_type_hint: Option<FileType>,
        skip_missing: bool,
    ) -> Result<Vec<InputContent>> {
        match *self {
            // TODO: should skip_missing also affect URLs?
            Input::RemoteUrl(ref url) => Ok(vec![Self::url_contents(url).await?]),
            Input::FsGlob {
                ref pattern,
                ignore_case,
            } => Ok(Self::glob_contents(pattern, ignore_case).await?),
            Input::FsPath(ref path) => {
                let content = Self::path_content(path).await;
                match content {
                    Ok(input_content) => Ok(vec![input_content]),
                    Err(_) if skip_missing => Ok(vec![]),
                    Err(e) => Err(e),
                }
            }
            Input::Stdin => Ok(vec![Self::stdin_content(file_type_hint).await?]),
            Input::String(ref s) => Ok(vec![Self::string_content(s, file_type_hint)]),
        }
    }

    async fn url_contents(url: &Url) -> Result<InputContent> {
        // Assume HTML for default paths
        let file_type = if url.path().is_empty() || url.path() == "/" {
            FileType::Html
        } else {
            FileType::from(url.as_str())
        };

        let res = reqwest::get(url.clone()).await?;
        let input_content = InputContent {
            input: Input::RemoteUrl(Box::new(url.clone())),
            file_type,
            content: res.text().await?,
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

    async fn path_content<P: Into<PathBuf> + AsRef<Path> + Clone>(path: P) -> Result<InputContent> {
        let content = read_to_string(&path)
            .await
            .map_err(|e| (path.clone().into(), e))?;
        let input_content = InputContent {
            file_type: FileType::from(path.as_ref()),
            content,
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

/// Collector keeps the state of link collection
#[derive(Debug, Clone)]
pub struct Collector {
    base_url: Option<Url>,
    skip_missing_inputs: bool,
    max_concurrency: usize,
    cache: HashSet<Uri>,
}

impl Collector {
    /// Create a new collector with an empty cache
    #[must_use]
    pub fn new(base_url: Option<Url>, skip_missing_inputs: bool, max_concurrency: usize) -> Self {
        Collector {
            base_url,
            skip_missing_inputs,
            max_concurrency,
            cache: HashSet::new(),
        }
    }

    /// Fetch all unique links from a slice of inputs
    /// All relative URLs get prefixed with `base_url` if given.
    ///
    /// # Errors
    ///
    /// Will return `Err` if links cannot be extracted from an input
    pub async fn collect_links(mut self, inputs: &[Input]) -> Result<HashSet<Request>> {
        let (contents_tx, mut contents_rx) = tokio::sync::mpsc::channel(self.max_concurrency);

        // extract input contents
        for input in inputs.iter().cloned() {
            let sender = contents_tx.clone();

            let skip_missing_inputs = self.skip_missing_inputs;
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
                let base_url = self.base_url.clone();
                let handle =
                    tokio::task::spawn_blocking(move || extract_links(&input_content, &base_url));
                extract_links_handles.push(handle);
            }
        }

        // Note: we could dispatch links to be checked as soon as we get them,
        //       instead of building a HashSet with all links.
        //       This optimization would speed up cases where there's
        //       a lot of inputs and/or the inputs are large (e.g. big files).
        let mut links: HashSet<Request> = HashSet::new();

        for handle in extract_links_handles {
            let new_links = handle.await?;
            links.extend(new_links);
        }

        // Filter out already cached links (duplicates)
        links.retain(|l| !self.cache.contains(&l.uri));

        self.update_cache(&links);
        Ok(links)
    }

    /// Update internal link cache
    fn update_cache(&mut self, links: &HashSet<Request>) {
        self.cache.extend(links.iter().cloned().map(|l| l.uri));
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Write};

    use http::StatusCode;
    use pretty_assertions::assert_eq;
    use reqwest::Url;

    use super::*;
    use crate::{
        extract::FileType,
        mock_server,
        test_utils::{mail, website},
        Result, Uri,
    };

    const TEST_STRING: &str = "http://test-string.com";
    const TEST_URL: &str = "https://test-url.org";
    const TEST_FILE: &str = "https://test-file.io";
    const TEST_GLOB_1: &str = "https://test-glob-1.io";
    const TEST_GLOB_2_MAIL: &str = "test@glob-2.io";

    #[tokio::test]
    #[ignore]
    async fn test_file_without_extension_is_plaintext() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        // Treat as plaintext file (no extension)
        let file_path = temp_dir.path().join("README");
        let _file = File::create(&file_path)?;
        let input = Input::new(&file_path.as_path().display().to_string(), true);
        let contents = input.get_contents(None, true).await?;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].file_type, FileType::Plaintext);
        Ok(())
    }

    #[tokio::test]
    async fn test_url_without_extension_is_html() -> Result<()> {
        let input = Input::new("https://example.org/", true);
        let contents = input.get_contents(None, true).await?;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].file_type, FileType::Html);
        Ok(())
    }

    #[tokio::test]
    async fn test_collect_links() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let temp_dir_path = temp_dir.path();

        let file_path = temp_dir_path.join("f");
        let file_glob_1_path = temp_dir_path.join("glob-1");
        let file_glob_2_path = temp_dir_path.join("glob-2");

        let mut file = File::create(&file_path)?;
        let mut file_glob_1 = File::create(file_glob_1_path)?;
        let mut file_glob_2 = File::create(file_glob_2_path)?;

        writeln!(file, "{}", TEST_FILE)?;
        writeln!(file_glob_1, "{}", TEST_GLOB_1)?;
        writeln!(file_glob_2, "{}", TEST_GLOB_2_MAIL)?;

        let mock_server = mock_server!(StatusCode::OK, set_body_string(TEST_URL));

        let inputs = vec![
            Input::String(TEST_STRING.to_owned()),
            Input::RemoteUrl(Box::new(
                Url::parse(&mock_server.uri()).map_err(|e| (mock_server.uri(), e))?,
            )),
            Input::FsPath(file_path),
            Input::FsGlob {
                pattern: temp_dir_path.join("glob*").to_str().unwrap().to_owned(),
                ignore_case: true,
            },
        ];

        let responses = Collector::new(None, false, 8)
            .collect_links(&inputs)
            .await?;
        let mut links = responses.into_iter().map(|r| r.uri).collect::<Vec<Uri>>();

        let mut expected_links = vec![
            website(TEST_STRING),
            website(TEST_URL),
            website(TEST_FILE),
            website(TEST_GLOB_1),
            mail(TEST_GLOB_2_MAIL),
        ];

        links.sort();
        expected_links.sort();
        assert_eq!(links, expected_links);

        Ok(())
    }
}
