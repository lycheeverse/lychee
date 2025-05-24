use crate::ErrorKind;
use crate::InputSource;
use crate::{
    Base, Input, Request, Result, basic_auth::BasicAuthExtractor, extract::Extractor,
    types::FileExtensions, types::uri::raw::RawUri, utils::request,
};
use futures::TryStreamExt;
use futures::{
    StreamExt,
    stream::{self, Stream},
};
use par_stream::ParStreamExt;
use std::path::PathBuf;

/// Collector keeps the state of link collection
/// It drives the link extraction from inputs
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct Collector {
    basic_auth_extractor: Option<BasicAuthExtractor>,
    skip_missing_inputs: bool,
    skip_ignored: bool,
    skip_hidden: bool,
    include_verbatim: bool,
    use_html5ever: bool,
    root_dir: Option<PathBuf>,
    base: Option<Base>,
}

impl Default for Collector {
    fn default() -> Self {
        Collector {
            basic_auth_extractor: None,
            skip_missing_inputs: false,
            include_verbatim: false,
            use_html5ever: false,
            skip_hidden: true,
            skip_ignored: true,
            root_dir: None,
            base: None,
        }
    }
}

impl Collector {
    /// Create a new collector with an empty cache
    ///
    /// # Errors
    ///
    /// Returns an `Err` if the `root_dir` is not an absolute path
    pub fn new(root_dir: Option<PathBuf>, base: Option<Base>) -> Result<Self> {
        if let Some(root_dir) = &root_dir {
            if root_dir.is_relative() {
                return Err(ErrorKind::RootDirMustBeAbsolute(root_dir.clone()));
            }
        }
        Ok(Collector {
            basic_auth_extractor: None,
            skip_missing_inputs: false,
            include_verbatim: false,
            use_html5ever: false,
            skip_hidden: true,
            skip_ignored: true,
            root_dir,
            base,
        })
    }

    /// Skip missing input files (default is to error if they don't exist)
    #[must_use]
    pub const fn skip_missing_inputs(mut self, yes: bool) -> Self {
        self.skip_missing_inputs = yes;
        self
    }

    /// Skip files that are hidden
    #[must_use]
    pub const fn skip_hidden(mut self, yes: bool) -> Self {
        self.skip_hidden = yes;
        self
    }

    /// Skip files that are ignored
    #[must_use]
    pub const fn skip_ignored(mut self, yes: bool) -> Self {
        self.skip_ignored = yes;
        self
    }

    /// Use `html5ever` to parse HTML instead of `html5gum`.
    #[must_use]
    pub const fn use_html5ever(mut self, yes: bool) -> Self {
        self.use_html5ever = yes;
        self
    }

    /// Skip over links in verbatim sections (like Markdown code blocks)
    #[must_use]
    pub const fn include_verbatim(mut self, yes: bool) -> Self {
        self.include_verbatim = yes;
        self
    }

    /// Pass a [`BasicAuthExtractor`] which is capable to match found
    /// URIs to basic auth credentials. These credentials get passed to the
    /// request in question.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn basic_auth_extractor(mut self, extractor: BasicAuthExtractor) -> Self {
        self.basic_auth_extractor = Some(extractor);
        self
    }

    /// Collect all sources from a list of [`Input`]s. For further details,
    /// see also [`Input::get_sources`](crate::Input#method.get_sources).
    pub fn collect_sources(self, inputs: Vec<Input>) -> impl Stream<Item = Result<String>> {
        stream::iter(inputs)
            .par_then_unordered(None, move |input| async move { input.get_sources() })
            .flatten()
    }

    /// Convenience method to fetch all unique links from inputs
    /// with the default extensions.
    pub fn collect_links(self, inputs: Vec<Input>) -> impl Stream<Item = Result<Request>> {
        self.collect_links_from_file_types(inputs, crate::types::FileType::default_extensions())
    }

    /// Fetch all unique links from inputs
    /// All relative URLs get prefixed with `base` (if given).
    /// (This can be a directory or a base URL)
    ///
    /// # Errors
    ///
    /// Will return `Err` if links cannot be extracted from an input
    pub fn collect_links_from_file_types(
        self,
        inputs: Vec<Input>,
        extensions: FileExtensions,
    ) -> impl Stream<Item = Result<Request>> {
        let skip_missing_inputs = self.skip_missing_inputs;
        let skip_hidden = self.skip_hidden;
        let skip_ignored = self.skip_ignored;
        let global_base = self.base;
        stream::iter(inputs)
            .par_then_unordered(None, move |input| {
                let default_base = global_base.clone();
                let extensions = extensions.clone();
                async move {
                    let base = match &input.source {
                        InputSource::RemoteUrl(url) => Base::try_from(url.as_str()).ok(),
                        _ => default_base,
                    };
                    input
                        .get_contents(skip_missing_inputs, skip_hidden, skip_ignored, extensions)
                        .map(move |content| (content, base.clone()))
                }
            })
            .flatten()
            .par_then_unordered(None, move |(content, base)| {
                let root_dir = self.root_dir.clone();
                let basic_auth_extractor = self.basic_auth_extractor.clone();
                async move {
                    let content = content?;
                    let extractor = Extractor::new(self.use_html5ever, self.include_verbatim);
                    let uris: Vec<RawUri> = extractor.extract(&content);
                    let requests = request::create(
                        uris,
                        &content.source,
                        root_dir.as_ref(),
                        base.as_ref(),
                        basic_auth_extractor.as_ref(),
                    );
                    Result::Ok(stream::iter(requests.into_iter().map(Ok)))
                }
            })
            .try_flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, convert::TryFrom, fs::File, io::Write};

    use http::{HeaderMap, StatusCode};
    use reqwest::Url;

    use super::*;
    use crate::{
        Result, Uri, mock_server,
        test_utils::{load_fixture, mail, path, website},
        types::{FileType, Input, InputSource},
    };

    // Helper function to run the collector on the given inputs
    async fn collect(
        inputs: Vec<Input>,
        root_dir: Option<PathBuf>,
        base: Option<Base>,
    ) -> Result<HashSet<Uri>> {
        let responses = Collector::new(root_dir, base)?.collect_links(inputs);
        Ok(responses.map(|r| r.unwrap().uri).collect().await)
    }

    /// Helper function for collecting verbatim links
    ///
    /// A verbatim link is a link that is not parsed by the HTML parser.
    /// For example, a link in a code block or a script tag.
    async fn collect_verbatim(
        inputs: Vec<Input>,
        root_dir: Option<PathBuf>,
        base: Option<Base>,
        extensions: FileExtensions,
    ) -> Result<HashSet<Uri>> {
        let responses = Collector::new(root_dir, base)?
            .include_verbatim(true)
            .collect_links_from_file_types(inputs, extensions);
        Ok(responses.map(|r| r.unwrap().uri).collect().await)
    }

    const TEST_STRING: &str = "http://test-string.com";
    const TEST_URL: &str = "https://test-url.org";
    const TEST_FILE: &str = "https://test-file.io";
    const TEST_GLOB_1: &str = "https://test-glob-1.io";
    const TEST_GLOB_2_MAIL: &str = "test@glob-2.io";

    #[tokio::test]
    async fn test_file_without_extension_is_plaintext() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        // Treat as plaintext file (no extension)
        let file_path = temp_dir.path().join("README");
        let _file = File::create(&file_path).unwrap();
        let input = Input::new(
            &file_path.as_path().display().to_string(),
            None,
            true,
            None,
            HeaderMap::new(),
        )?;
        let contents: Vec<_> = input
            .get_contents(true, true, true, FileType::default_extensions())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].as_ref().unwrap().file_type, FileType::Plaintext);
        Ok(())
    }

    #[tokio::test]
    async fn test_url_without_extension_is_html() -> Result<()> {
        let input = Input::new("https://example.com/", None, true, None, HeaderMap::new())?;
        let contents: Vec<_> = input
            .get_contents(true, true, true, FileType::default_extensions())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].as_ref().unwrap().file_type, FileType::Html);
        Ok(())
    }

    #[tokio::test]
    async fn test_collect_links() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();

        let file_path = temp_dir_path.join("f");
        let file_glob_1_path = temp_dir_path.join("glob-1");
        let file_glob_2_path = temp_dir_path.join("glob-2");

        let mut file = File::create(&file_path).unwrap();
        let mut file_glob_1 = File::create(file_glob_1_path).unwrap();
        let mut file_glob_2 = File::create(file_glob_2_path).unwrap();

        writeln!(file, "{TEST_FILE}").unwrap();
        writeln!(file_glob_1, "{TEST_GLOB_1}").unwrap();
        writeln!(file_glob_2, "{TEST_GLOB_2_MAIL}").unwrap();

        let mock_server = mock_server!(StatusCode::OK, set_body_string(TEST_URL));

        let inputs = vec![
            Input {
                source: InputSource::String(TEST_STRING.to_owned()),
                file_type_hint: None,
                excluded_paths: None,
                headers: HeaderMap::new(),
            },
            Input {
                source: InputSource::RemoteUrl(Box::new(
                    Url::parse(&mock_server.uri())
                        .map_err(|e| (mock_server.uri(), e))
                        .unwrap(),
                )),
                file_type_hint: None,
                excluded_paths: None,
                headers: HeaderMap::new(),
            },
            Input {
                source: InputSource::FsPath(file_path),
                file_type_hint: None,
                excluded_paths: None,
                headers: HeaderMap::new(),
            },
            Input {
                source: InputSource::FsGlob {
                    pattern: temp_dir_path.join("glob*").to_str().unwrap().to_owned(),
                    ignore_case: true,
                },
                file_type_hint: None,
                excluded_paths: None,
                headers: HeaderMap::new(),
            },
        ];

        let links = collect_verbatim(inputs, None, None, FileType::default_extensions())
            .await
            .ok()
            .unwrap();

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
            excluded_paths: None,
            headers: HeaderMap::new(),
        };
        let links = collect(vec![input], None, Some(base)).await.ok().unwrap();

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
            excluded_paths: None,
            headers: HeaderMap::new(),
        };
        let links = collect(vec![input], None, Some(base)).await.ok().unwrap();

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
            excluded_paths: None,
            headers: HeaderMap::new(),
        };
        let links = collect(vec![input], None, Some(base)).await.ok().unwrap();

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
                "This is [an internal url](@/internal.md)
        This is [an internal url](@/internal.markdown)
        This is [an internal url](@/internal.markdown#example)
        This is [an internal url](@/internal.md#example)"
                    .to_string(),
            ),
            file_type_hint: Some(FileType::Markdown),
            excluded_paths: None,
            headers: HeaderMap::new(),
        };

        let links = collect(vec![input], None, Some(base)).await.ok().unwrap();

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
        let base = Base::try_from("https://example.com").unwrap();
        let input = load_fixture("TEST_HTML5.html");

        let input = Input {
            source: InputSource::String(input),
            file_type_hint: Some(FileType::Html),
            excluded_paths: None,
            headers: HeaderMap::new(),
        };
        let links = collect(vec![input], None, Some(base)).await.ok().unwrap();

        let expected_links = HashSet::from_iter([
            // the body links wouldn't be present if the file was parsed strictly as XML
            website("https://example.com/body/a"),
            website("https://example.com/body/div_empty_a"),
            website("https://example.com/css/style_full_url.css"),
            website("https://example.com/css/style_relative_url.css"),
            website("https://example.com/head/home"),
            website("https://example.com/images/icon.png"),
        ]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_relative_url_with_base_extracted_from_input() {
        let contents = r#"<html>
            <div class="row">
                <a href="https://github.com/lycheeverse/lychee/">GitHub</a>
                <a href="/about">About</a>
            </div>
        </html>"#;
        let mock_server = mock_server!(StatusCode::OK, set_body_string(contents));

        let server_uri = Url::parse(&mock_server.uri()).unwrap();

        let input = Input {
            source: InputSource::RemoteUrl(Box::new(server_uri.clone())),
            file_type_hint: None,
            excluded_paths: None,
            headers: HeaderMap::new(),
        };

        let links = collect(vec![input], None, None).await.ok().unwrap();

        let expected_urls = HashSet::from_iter([
            website("https://github.com/lycheeverse/lychee/"),
            website(&format!("{server_uri}about")),
        ]);

        assert_eq!(links, expected_urls);
    }

    #[tokio::test]
    async fn test_email_with_query_params() {
        let input = Input {
            source: InputSource::String(
                "This is a mailto:user@example.com?subject=Hello link".to_string(),
            ),
            file_type_hint: None,
            excluded_paths: None,
            headers: HeaderMap::new(),
        };
        let links = collect(vec![input], None, None).await.ok().unwrap();

        let expected_links = HashSet::from_iter([mail("user@example.com")]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_multiple_remote_urls() {
        let mock_server_1 = mock_server!(
            StatusCode::OK,
            set_body_string(r#"<a href="relative.html">Link</a>"#)
        );
        let mock_server_2 = mock_server!(
            StatusCode::OK,
            set_body_string(r#"<a href="relative.html">Link</a>"#)
        );

        let inputs = vec![
            Input {
                source: InputSource::RemoteUrl(Box::new(
                    Url::parse(&format!(
                        "{}/foo/index.html",
                        mock_server_1.uri().trim_end_matches('/')
                    ))
                    .unwrap(),
                )),
                file_type_hint: Some(FileType::Html),
                excluded_paths: None,
                headers: HeaderMap::new(),
            },
            Input {
                source: InputSource::RemoteUrl(Box::new(
                    Url::parse(&format!(
                        "{}/bar/index.html",
                        mock_server_2.uri().trim_end_matches('/')
                    ))
                    .unwrap(),
                )),
                file_type_hint: Some(FileType::Html),
                excluded_paths: None,
                headers: HeaderMap::new(),
            },
        ];

        let links = collect(inputs, None, None).await.ok().unwrap();

        let expected_links = HashSet::from_iter([
            website(&format!(
                "{}/foo/relative.html",
                mock_server_1.uri().trim_end_matches('/')
            )),
            website(&format!(
                "{}/bar/relative.html",
                mock_server_2.uri().trim_end_matches('/')
            )),
        ]);

        assert_eq!(links, expected_links);
    }

    #[tokio::test]
    async fn test_file_path_with_base() {
        let base = Base::try_from("/path/to/root").unwrap();
        assert_eq!(base, Base::Local("/path/to/root".into()));

        let input = Input {
            source: InputSource::String(
                r#"
                <a href="index.html">Index</a>
                <a href="about.html">About</a> 
                <a href="/another.html">Another</a> 
            "#
                .into(),
            ),
            file_type_hint: Some(FileType::Html),
            excluded_paths: None,
            headers: HeaderMap::new(),
        };

        let links = collect(vec![input], None, Some(base)).await.ok().unwrap();

        let expected_links = HashSet::from_iter([
            path("/path/to/root/index.html"),
            path("/path/to/root/about.html"),
            path("/another.html"),
        ]);

        assert_eq!(links, expected_links);
    }
}
