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
}

impl Collector {
    /// Create a new collector with an empty cache
    #[must_use]
    pub const fn new(base: Option<Base>, skip_missing_inputs: bool) -> Self {
        Collector {
            base,
            skip_missing_inputs,
        }
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
                input.get_contents(None, skip_missing_inputs).await
            })
            .flatten();

        let base = self.base;
        contents
            .par_then_unordered(None, move |content| {
                // send to parallel worker
                let base = base.clone();
                async move {
                    let content = content?;
                    let uris: Vec<RawUri> = Extractor::extract(&content);
                    let requests = request::create(uris, &content, &base)?;
                    Result::Ok(stream::iter(requests.into_iter().map(Ok)))
                }
            })
            .try_flatten()
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
        types::{FileType, Input, InputSource},
        Result, Uri,
    };

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
        let contents: Vec<_> = input
            .get_contents(None, true)
            .await
            .collect::<Vec<_>>()
            .await;

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].as_ref().unwrap().file_type, FileType::Plaintext);
        Ok(())
    }

    #[tokio::test]
    async fn test_url_without_extension_is_html() -> Result<()> {
        let input = Input::new("https://example.org/", None, true);
        let contents: Vec<_> = input
            .get_contents(None, true)
            .await
            .collect::<Vec<_>>()
            .await;

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

        let responses = Collector::new(None, false).collect_links(inputs).await;
        let mut links: Vec<Uri> = responses.map(|r| r.unwrap().uri).collect().await;

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

    // #[test]
    // fn test_extract_markdown_links() {
    //     let base = Some("https://github.com/hello-rust/lychee/");
    //     let links = extract_uris(
    //         "This is [a test](https://endler.dev). This is a relative link test [Relative Link Test](relative_link)",
    //         FileType::Markdown,
    //     );
    //     let responses = Collector::new(base, false).collect_links(inputs).await;
    //     let mut links: Vec<Uri> = responses.map(|r| r.unwrap().uri).collect().await;

    //     let expected_links = array::IntoIter::new([
    //         website("https://endler.dev"),
    //         website("https://github.com/hello-rust/lychee/relative_link"),
    //     ])
    //     .collect::<HashSet<Uri>>();

    //     assert_eq!(links, expected_links);
    // }

    // #[test]
    // fn test_extract_html_links() {
    //     let links = extract_uris(
    //         r#"<html>
    //             <div class="row">
    //                 <a href="https://github.com/lycheeverse/lychee/">
    //                 <a href="blob/master/README.md">README</a>
    //             </div>
    //         </html>"#,
    //         FileType::Html,
    //         Some("https://github.com/lycheeverse/"),
    //     );

    //     let expected_links = array::IntoIter::new([
    //         website("https://github.com/lycheeverse/lychee/"),
    //         website("https://github.com/lycheeverse/blob/master/README.md"),
    //     ])
    //     .collect::<HashSet<Uri>>();

    //     assert_eq!(links, expected_links);
    // }

    // #[test]
    // fn test_extract_html_srcset() {
    //     let links = extract_uris(
    //         r#"
    //         <img
    //             src="/static/image.png"
    //             srcset="
    //             /static/image300.png  300w,
    //             /static/image600.png  600w,
    //             "
    //         />
    //       "#,
    //         FileType::Html,
    //         Some("https://example.com/"),
    //     );
    //     let expected_links = array::IntoIter::new([
    //         website("https://example.com/static/image.png"),
    //         website("https://example.com/static/image300.png"),
    //         website("https://example.com/static/image600.png"),
    //     ])
    //     .collect::<HashSet<Uri>>();

    //     assert_eq!(links, expected_links);
    // }

    // #[test]
    // fn test_markdown_internal_url() {
    //     let base_url = "https://localhost.com/";
    //     let input = "This is [an internal url](@/internal.md) \
    //     This is [an internal url](@/internal.markdown) \
    //     This is [an internal url](@/internal.markdown#example) \
    //     This is [an internal url](@/internal.md#example)";

    //     let links = extract_uris(input, FileType::Markdown, Some(base_url));

    //     let expected = array::IntoIter::new([
    //         website("https://localhost.com/@/internal.md"),
    //         website("https://localhost.com/@/internal.markdown"),
    //         website("https://localhost.com/@/internal.md#example"),
    //         website("https://localhost.com/@/internal.markdown#example"),
    //     ])
    //     .collect::<HashSet<Uri>>();

    //     assert_eq!(links, expected);
    // }

    // #[test]
    // fn test_extract_html5_not_valid_xml_relative_links() {
    //     let input = load_fixture("TEST_HTML5.html");
    //     let links = extract_uris(&input, FileType::Html, Some("https://example.org"));

    //     let expected_links = HashSet::from_iter([
    //         // the body links wouldn't be present if the file was parsed strictly as XML
    //         website("https://example.org/body/a"),
    //         website("https://example.org/body/div_empty_a"),
    //         website("https://example.org/css/style_full_url.css"),
    //         website("https://example.org/css/style_relative_url.css"),
    //         website("https://example.org/head/home"),
    //         website("https://example.org/images/icon.png"),
    //         website("https://example.org/js/script.js"),
    //     ]);

    //     assert_eq!(links, expected_links);
    // }

    // #[test]
    // fn test_relative_url_with_base_extracted_from_input() {
    //     let input = Input::RemoteUrl(Box::new(
    //         Url::parse("https://example.org/some-post").unwrap(),
    //     ));

    //     let contents = r#"<html>
    //         <div class="row">
    //             <a href="https://github.com/lycheeverse/lychee/">Github</a>
    //             <a href="/about">About</a>
    //         </div>
    //     </html>"#;

    //     let input_content = &InputContent {
    //         input,
    //         file_type: FileType::Html,
    //         content: contents.to_string(),
    //     };

    //     let links = Extractor::extract(input_content);
    //     let urls = links
    //         .into_iter()
    //         .map(|raw_uri| raw_uri.text)
    //         .collect::<HashSet<_>>();

    //     let expected_urls = array::IntoIter::new([
    //         String::from("https://github.com/lycheeverse/lychee/"),
    //         String::from("/about"),
    //     ])
    //     .collect::<HashSet<_>>();

    //     assert_eq!(urls, expected_urls);
    // }
}
