use lychee_lib::{Collector, Input, Result};
use reqwest::Url;
use std::path::PathBuf;

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // Collect all links from the following inputs
    let inputs: &[Input] = &[
        Input::RemoteUrl(Box::new(
            Url::parse("https://github.com/lycheeverse/lychee").unwrap(),
        )),
        Input::FsPath(PathBuf::from("fixtures/TEST.md")),
    ];

    let links = Collector::new(
        None, // base_url
        None, false, // don't skip missing inputs
        10,    // max concurrency
    )
    .collect_links(
        inputs, // base_url
    )
    .await?;

    dbg!(links);

    Ok(())
}
