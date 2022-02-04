use crate::{
    extract::Extractor, helpers::request, types::raw_uri::RawUri, Base, Input, Request, Result,
};
use futures::{
    stream::{self, Stream},
    StreamExt, TryStreamExt,
};
use par_stream::ParStreamExt;

/// Collector keeps the state of link collection
/// It drives the link extraction from inputs
#[derive(Debug, Clone)]
pub struct Collector {
    base: Option<Base>,
    skip_missing_inputs: bool,
    use_html5ever: bool,
}

impl Collector {
    /// Create a new collector with an empty cache
    #[must_use]
    pub const fn new(base: Option<Base>) -> Self {
        Collector {
            base,
            skip_missing_inputs: false,
            use_html5ever: false,
        }
    }

    /// Skip missing input files (default is to error if they don't exist)
    #[must_use]
    pub const fn skip_missing_inputs(mut self, yes: bool) -> Self {
        self.skip_missing_inputs = yes;
        self
    }

    /// Use `html5ever` to parse HTML instead of `html5gum`.
    #[must_use]
    pub const fn use_html5ever(mut self, yes: bool) -> Self {
        self.use_html5ever = yes;
        self
    }

    /// Fetch all unique links from inputs
    /// All relative URLs get prefixed with `base` (if given).
    /// (This can be a directory or a base URL)
    ///
    /// # Errors
    ///
    /// Will return `Err` if links cannot be extracted from an input
    pub async fn collect_links(self, inputs: Vec<Input>) -> impl Stream<Item = Result<Request>> {
        let skip_missing_inputs = self.skip_missing_inputs;
        let contents = stream::iter(inputs)
            .par_then_unordered(None, move |input| async move {
                input.get_contents(skip_missing_inputs).await
            })
            .flatten();

        let base = self.base;
        contents
            .par_then_unordered(None, move |content| {
                // send to parallel worker
                let base = base.clone();
                async move {
                    let content = content?;
                    let uris: Vec<RawUri> = if self.use_html5ever {
                        Extractor::extract_html5ever(&content)
                    } else {
                        Extractor::extract(&content)
                    };
                    let requests = request::create(uris, &content, &base)?;
                    Result::Ok(stream::iter(requests.into_iter().map(Ok)))
                }
            })
            .try_flatten()
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashSet, convert::TryFrom, fs::File, io::Write};

    use http::StatusCode;
    use pretty_assertions::assert_eq;
    use reqwest::Url;

    use super::*;
    use crate::{
        mock_server,
        test_utils::{load_fixture, mail, website},
        types::{FileType, Input, InputSource},
        Result, Uri,
    };

    // Helper function to run the collector on the given inputs
    async fn collect(inputs: Vec<Input>, base: Option<Base>) -> HashSet<Uri> {
        let responses = Collector::new(base).collect_links(inputs).await;
        responses.map(|r| r.unwrap().uri).collect().await
    }

    const TEST_STRING: &str = "http://test-string.com";
    const TEST_URL: &str = "https://test-url.org";
    const TEST_FILE: &str = "https://test-file.io";
    const TEST_GLOB_1: &str = "https://test-glob-1.io";
    const TEST_GLOB_2_MAIL: &str = "test@glob-2.io";

    #[tokio::test]
    async fn test_file_without_extension_is_plaintext() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        // Treat as plaintext file (no extension)
        let file_path = temp_dir.path().join("README");
        let _file = File::create(&file_path)?;
        let input = Input::new(&file_path.as_path().display().to_string(), None, true);
        let contents: Vec<_> = input.get_contents(true).await.collect::<Vec<_>>().await;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].as_ref().unwrap().file_type, FileType::Plaintext);
        Ok(())
    }

    #[tokio::test]
    async fn test_url_without_extension_is_html() -> Result<()> {
        let input = Input::new("https://example.org/", None, true);
        let contents: Vec<_> = input.get_contents(true).await.collect::<Vec<_>>().await;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].as_ref().unwrap().file_type, FileType::Html);
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
            Input {
                source: InputSource::String(TEST_STRING.to_owned()),
                file_type_hint: None,
            },
            Input {
                source: InputSource::RemoteUrl(Box::new(
                    Url::parse(&mock_server.uri()).map_err(|e| (mock_server.uri(), e))?,
                )),
                file_type_hint: None,
            },
            Input {
                source: InputSource::FsPath(file_path),
                file_type_hint: None,
            },
            Input {
                source: InputSource::FsGlob {
                    pattern: temp_dir_path.join("glob*").to_str().unwrap().to_owned(),
                    ignore_case: true,
                },
                file_type_hint: None,
            },
        ];

        let links = collect(inputs, None).await;

        let expected_links = HashSet::from_iter([
            website(TEST_STRING),
            website(TEST_URL),
            website(TEST_FILE),
            website(TEST_GLOB_1),
            mail(TEST_GLOB_2_MAIL),
        ]);

        assert_eq!(links, expected_links);

        Ok(())
    }

    #[tokio::test]
    async fn test_collect_markdown_links() {
        let base = Base::try_from("https://github.com/hello-rust/lychee/").unwrap();
        let input = Input {
            source: InputSource::String("This is [a test](https://endler.dev). This is a relative link test [Relative Link Test](relative_link)".to_string()),
            file_type_hint: Some(FileType::Markdown),
        };
        let links = collect(vec![input], Some(base)).await;

        let expected_links = HashSet::from_iter([
            website("https://endler.dev"),
            website("https://github.com/hello-rust/lychee/relative_link"),
        ]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_collect_html_links() {
        let base = Base::try_from("https://github.com/lycheeverse/").unwrap();
        let input = Input {
            source: InputSource::String(
                r#"<html>
                <div class="row">
                    <a href="https://github.com/lycheeverse/lychee/">
                    <a href="blob/master/README.md">README</a>
                </div>
            </html>"#
                    .to_string(),
            ),
            file_type_hint: Some(FileType::Html),
        };
        let links = collect(vec![input], Some(base)).await;

        let expected_links = HashSet::from_iter([
            website("https://github.com/lycheeverse/lychee/"),
            website("https://github.com/lycheeverse/blob/master/README.md"),
        ]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_collect_html_srcset() {
        let base = Base::try_from("https://example.com/").unwrap();
        let input = Input {
            source: InputSource::String(
                r#"
            <img
                src="/static/image.png"
                srcset="
                /static/image300.png  300w,
                /static/image600.png  600w,
                "
            />
          "#
                .to_string(),
            ),
            file_type_hint: Some(FileType::Html),
        };
        let links = collect(vec![input], Some(base)).await;

        let expected_links = HashSet::from_iter([
            website("https://example.com/static/image.png"),
            website("https://example.com/static/image300.png"),
            website("https://example.com/static/image600.png"),
        ]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_markdown_internal_url() {
        let base = Base::try_from("https://localhost.com/").unwrap();

        let input = Input {
            source: InputSource::String(
                r#"This is [an internal url](@/internal.md)
        This is [an internal url](@/internal.markdown)
        This is [an internal url](@/internal.markdown#example)
        This is [an internal url](@/internal.md#example)"#
                    .to_string(),
            ),
            file_type_hint: Some(FileType::Markdown),
        };

        let links = collect(vec![input], Some(base)).await;

        let expected = HashSet::from_iter([
            website("https://localhost.com/@/internal.md"),
            website("https://localhost.com/@/internal.markdown"),
            website("https://localhost.com/@/internal.md#example"),
            website("https://localhost.com/@/internal.markdown#example"),
        ]);

        assert_eq!(links, expected);
    }

    #[tokio::test]
    async fn test_extract_html5_not_valid_xml_relative_links() {
        let base = Base::try_from("https://example.org").unwrap();
        let input = load_fixture("TEST_HTML5.html");

        let input = Input {
            source: InputSource::String(input),
            file_type_hint: Some(FileType::Html),
        };
        let links = collect(vec![input], Some(base)).await;

        let expected_links = HashSet::from_iter([
            // the body links wouldn't be present if the file was parsed strictly as XML
            website("https://example.org/body/a"),
            website("https://example.org/body/div_empty_a"),
            website("https://example.org/css/style_full_url.css"),
            website("https://example.org/css/style_relative_url.css"),
            website("https://example.org/head/home"),
            website("https://example.org/images/icon.png"),
            website("https://example.org/js/script.js"),
        ]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_relative_url_with_base_extracted_from_input() {
        let contents = r#"<html>
            <div class="row">
                <a href="https://github.com/lycheeverse/lychee/">Github</a>
                <a href="/about">About</a>
            </div>
        </html>"#;
        let mock_server = mock_server!(StatusCode::OK, set_body_string(contents));

        let server_uri = Url::parse(&mock_server.uri()).unwrap();

        let input = Input {
            source: InputSource::RemoteUrl(Box::new(server_uri.clone())),
            file_type_hint: None,
        };

        let links = collect(vec![input], None).await;

        let expected_urls = HashSet::from_iter([
            website("https://github.com/lycheeverse/lychee/"),
            website(&format!("{}about", server_uri)),
        ]);

        assert_eq!(links, expected_urls);
    }
}
