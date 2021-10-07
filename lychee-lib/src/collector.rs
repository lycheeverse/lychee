use crate::{extract::Extractor, Base, Input, Request, Result};
use std::collections::HashSet;

/// Collector keeps the state of link collection
#[derive(Debug, Clone)]
pub struct Collector {
    base: Option<Base>,
    skip_missing_inputs: bool,
    max_concurrency: usize,
}

impl Collector {
    /// Create a new collector with an empty cache
    #[must_use]
    pub const fn new(
        base: Option<Base>,
        skip_missing_inputs: bool,
        max_concurrency: usize,
    ) -> Self {
        Collector {
            base,
            skip_missing_inputs,
            max_concurrency,
        }
    }

    /// Fetch all unique links from a slice of inputs
    /// All relative URLs get prefixed with `base` if given.
    /// (This can be a directory or a base URL)
    ///
    /// # Errors
    ///
    /// Will return `Err` if links cannot be extracted from an input
    pub async fn collect_links(self, inputs: &[Input]) -> Result<HashSet<Request>> {
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
                let base = self.base.clone();
                let handle = tokio::task::spawn_blocking(move || {
                    let mut extractor = Extractor::new(base);
                    extractor.extract(&input_content)
                });
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
            links.extend(new_links?);
        }

        Ok(links)
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
        mock_server,
        test_utils::{mail, website},
        types::{FileType, Input},
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
