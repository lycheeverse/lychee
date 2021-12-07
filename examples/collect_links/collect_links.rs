use lychee_lib::{Collector, Input, Result};
use reqwest::Url;
use std::path::PathBuf;
use tokio_stream::StreamExt;

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // Collect all links from the following inputs
    let inputs = vec![
        Input::RemoteUrl(Box::new(
            Url::parse("https://github.com/lycheeverse/lychee").unwrap(),
        )),
        Input::FsPath(PathBuf::from("fixtures/TEST.md")),
    ];

    let links = Collector::new(
        None,  // base
        false, // don't skip missing inputs
    )
    .collect_links(
        inputs, // base url or directory
    )
    .await
    .collect::<Result<Vec<_>>>()
    .await?;

    dbg!(links);

    Ok(())
}
