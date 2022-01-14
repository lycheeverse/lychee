/// Example of a collector which receives inputs from a channel (instead of a vector).
/// With that, inputs can be fed into the collector while it is running.
/// This also serves as a demonstration of how lychee handles recursion.
/// 
/// If you don't care about recursion, the `collect_links` example might be more convenient.
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
        },
        Input {
            source: InputSource::FsPath(PathBuf::from("fixtures/TEST.md")),
            file_type_hint: None,
        },
    ];

    let links = Collector::new(
        None,  // base
        false, // don't skip missing inputs
    )
    .from_iter(inputs)
    .await
    .collect::<Result<Vec<_>>>()
    .await?;

    dbg!(links);

    Ok(())
}
