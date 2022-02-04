use lychee_lib::{Collector, Input, InputSource, Result};
use reqwest::Url;
use std::path::PathBuf;
use tokio_stream::StreamExt;

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // Collect all links from the following inputs
    let inputs = [
        Input {
            source: InputSource::RemoteUrl(Box::new(
                Url::parse("https://github.com/lycheeverse/lychee").unwrap(),
            )),
            file_type_hint: None,
            recursion_level: 0,
        },
        Input {
            source: InputSource::FsPath(PathBuf::from("fixtures/TEST.md")),
            file_type_hint: None,
            recursion_level: 0,
        },
    ];

    let links = Collector::from_iter(
        None,  // base
        false, // don't skip missing inputs
        inputs,
    )
    .await
    .collect::<Result<Vec<_>>>()
    .await?;

    dbg!(links);

    Ok(())
}
