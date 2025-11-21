use lychee_lib::{Collector, Input, InputSource, RequestError};
use reqwest::Url;
use std::{collections::HashSet, path::PathBuf};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<RequestError>> {
    // Collect all links from the following inputs
    let inputs = HashSet::from_iter([
        Input::from_input_source(InputSource::RemoteUrl(Box::new(
            Url::parse("https://github.com/lycheeverse/lychee").unwrap(),
        ))),
        Input::from_input_source(InputSource::FsPath(PathBuf::from("fixtures/TEST.md"))),
    ]);

    let links = Collector::default()
        .skip_missing_inputs(false) // don't skip missing inputs? (default=false)
        .skip_hidden(false) // skip hidden files? (default=true)
        .skip_ignored(false) // skip files that are ignored by git? (default=true)
        .use_html5ever(false) // use html5ever for parsing? (default=false)
        .collect_links(inputs) // base url or directory
        .collect::<Result<Vec<_>, _>>()
        .await?;

    dbg!(links);

    Ok(())
}
