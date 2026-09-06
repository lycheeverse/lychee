#[cfg(test)]
mod github_fragments {

    use std::error::Error;

    use lychee_lib::extract::fragments::generate_without_disambiguation;
    use tempfile::tempdir;
    use test_utils::fixtures_path;

    const API_URL: &str = "https://api.github.com/repos/lycheeverse/lychee/contents";
    const FILE_PATH: &str = "/fixtures/fragments/special-casing.md";
    const COMMIT: &str = "0788e393989c4f1f747529324189a8a74d6f2e96";

    async fn convert_special_casing_txt_to_markdown() -> Result<String, reqwest::Error> {
        let resp =
            reqwest::get("https://www.unicode.org/Public/17.0.0/ucd/SpecialCasing.txt").await?;
        let text = resp.text().await?;

        // Exclude Language-Sensitive Mappings by keeping only rules before that
        // section heading.
        let text = text.split_once("Language-Sensitive Mappings").unwrap().0;

        // Turn contiguous commented regions beginning with '#' into Markdown code blocks.
        // This is just to give the context for each special casing rule and doesn't affect the test itself.
        let begin_contiguous_comment = regex::Regex::new(r"(?m)^#[^\n]*\n\n").unwrap();
        let end_contiguous_comment = regex::Regex::new(r"\n\n#").unwrap();
        let text = begin_contiguous_comment.replace_all(text, |captures: &regex::Captures| {
            format!("{}\n```\n\n", captures.get_match().as_str().trim_end())
        });
        let text = end_contiguous_comment.replace_all(&text, "\n\n```\n#");

        // Turn lines *not* beginning with backtick or # into Markdown headings.
        let heading_lines = regex::Regex::new(r"(?m)^[^`#\n]").unwrap();
        let text = heading_lines.replace_all(&text, |captures: &regex::Captures| {
            format!("# {}", captures.get_match().as_str())
        });

        // Turn Unicode codepoints (4 hex digits) into HTML entities and make the
        // codepoints adjacent to trigger special casing.
        let four_hex_digits = regex::Regex::new(r"[0-9A-F]{4}").unwrap();
        let text = four_hex_digits.replace_all(&text, |captures: &regex::Captures| {
            format!("&#x{};", captures.get_match().as_str())
        });
        let text = text.replace("; &", ";&");
        let text = text.replace(";;", ";-");

        Ok(format!("```\n{text}\n```\n"))
    }

    /// Given HTML rendered by the Github API, this will extract
    /// `(title, expected_fragment)` pairs.
    ///
    /// The HTML input has this format for Markdown fragment headings
    /// (line breaks added for clarity and are not in the original HTML).
    ///
    /// ```html
    /// <div class="markdown-heading" dir="auto">
    ///   <h1 class="heading-element" dir="auto">ß-ß-Ss-SS- # LATIN SMALL LETTER SHARP S</h1>
    ///   <a id="..." class="anchor" aria-label="..." href="#ß-ß-ss-ss---latin-small-letter-sharp-s">
    ///   ...
    /// ```
    fn extract_special_casing_html(html: &str) -> impl std::iter::Iterator<Item = (&str, &str)> {
        fn extract_line(l: &str) -> (&str, &str) {
            let title = l.split_once("</h1>").unwrap().0.rsplit_once('>').unwrap().1;
            let href = "href=\"#";
            let id = l.split_once(href).unwrap().1.split_once('"').unwrap().0;
            (title, id)
        }

        html.lines()
            .filter(|l| l.contains("class=\"markdown-heading\""))
            .map(extract_line)
    }

    fn github_request_builder(accept: &str) -> reqwest::RequestBuilder {
        let mut auth_header = reqwest::header::HeaderMap::new();
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            let token = format!("Bearer {token}").try_into().unwrap();
            auth_header.insert("Authorization", token).unwrap();
        }

        let client = reqwest::Client::new();
        client
            .get(format!("{API_URL}{FILE_PATH}?ref={COMMIT}"))
            .header("Accept", accept)
            .header("X-GitHub-Api-Version", "2026-03-10")
            .header("User-Agent", "Lychee-Unit-Test")
            .headers(auth_header)
    }

    /// Tests by using the Github API to render the HTML of a Markdown file
    /// which has been committed to the repo.
    ///
    /// See `fixtures/fragments/make-special-casing.md` for how to generate that
    /// markdown file.
    #[tokio::test]
    async fn test_github_fragments_live() -> Result<(), reqwest::Error> {
        let resp = github_request_builder("application/vnd.github.html+json")
            .send()
            .await;

        match resp.and_then(reqwest::Response::error_for_status) {
            Ok(resp) => {
                extract_special_casing_html(&resp.text().await?).for_each(|(title, expected)| {
                    assert_eq!(expected, generate_without_disambiguation(title));
                });
                Ok(())
            }
            Err(err)
                if err
                    .status()
                    .is_some_and(|c| c == http::StatusCode::TOO_MANY_REQUESTS) =>
            {
                println!("Ignoring 429 in live Github fragment slugify test");
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn special_casing_md_path() -> std::path::PathBuf {
        fixtures_path!().join("fragments/special-casing.md")
    }

    fn load_special_casing_md() -> Result<String, std::io::Error> {
        let actual =
            std::fs::read_to_string(special_casing_md_path()).or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(String::new()),
                _ => Err(e),
            })?;

        Ok(actual.replace("\r\n", "\n"))
    }

    /// Tests that the [`COMMIT`] and [`FILE_PATH`] references a file which is identical
    /// to the in-repo `special-casing.md`.
    #[tokio::test]
    async fn test_remote_special_casing_md() -> Result<(), Box<dyn Error>> {
        let expected = load_special_casing_md()?;

        let resp = github_request_builder("application/vnd.github.raw+json")
            .send()
            .await;

        match resp.and_then(reqwest::Response::error_for_status) {
            Ok(resp) => {
                assert!(
                    resp.text().await? == expected,
                    "Local and remote special-casing.md don't match! If \
                        `test_local_special_casing_md` is also failing, fix that first. \
                        Otherwise, push the local special-casing.md to Github \
                        and update the `COMMIT` in this test module."
                );
                Ok(())
            }
            Err(err)
                if err
                    .status()
                    .is_some_and(|c| c == http::StatusCode::TOO_MANY_REQUESTS) =>
            {
                println!("Ignoring 429 in live Github fragment slugify test");
                Ok(())
            }
            Err(err) => Err(err)?,
        }
    }

    /// Tests that the markdown file in repo has the correct format and matches
    /// the output of [`convert_special_casing_txt_to_markdown`].
    #[tokio::test]
    async fn test_local_special_casing_md() -> Result<(), Box<dyn Error>> {
        let actual = load_special_casing_md()?;
        let expected = convert_special_casing_txt_to_markdown().await?;

        // If not equal, write to a tempdir. The strings are too
        // big to be readable in `assert_eq!`.
        if expected != actual {
            let mut temp = tempdir().unwrap();
            temp.disable_cleanup(true);

            let expected_path = temp.path().join("expected.md");
            let actual_path = temp.path().join("actual.md");
            println!(
                "In-repo special-casing.md does not match! Writing \
                 expected and actual contents to: {}",
                temp.path().to_string_lossy()
            );
            println!(
                "\nTo inspect:\n    diff -u {} {}",
                actual_path.to_string_lossy(),
                expected_path.to_string_lossy(),
            );
            println!(
                "\nTo apply:\n    cp -v {} {}\n",
                expected_path.to_string_lossy(),
                special_casing_md_path().to_string_lossy(),
            );

            std::fs::write(expected_path, &expected).unwrap();
            std::fs::write(actual_path, &actual).unwrap();
        }
        assert!(expected == actual);
        Ok(())
    }
}
