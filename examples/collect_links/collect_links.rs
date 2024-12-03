use lychee_lib::{Collector, Input, InputSource, Result};
use reqwest::Url;
use std::path::PathBuf;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    // Collect all links from the following inputs
    let inputs = vec![
        Input {
            source: InputSource::RemoteUrl(Box::new(
                Url::parse("https://github.com/lycheeverse/lychee").unwrap(),
            )),
            file_type_hint: None,
            excluded_paths: None,
        },
        Input {
            source: InputSource::FsPath(PathBuf::from("fixtures/TEST.md")),
            file_type_hint: None,
            excluded_paths: None,
        },
    ];

    let links = Collector::default() // root_path and base
        .skip_missing_inputs(false) // don't skip missing inputs? (default=false)
        .skip_hidden(false) // skip hidden files? (default=true)
        .skip_ignored(false) // skip files that are ignored by git? (default=true)
        .use_html5ever(false) // use html5ever for parsing? (default=false)
        .collect_links(inputs) // base url or directory
        .collect::<Result<Vec<_>>>()
        .await?;

    dbg!(links);

    Ok(())
}
