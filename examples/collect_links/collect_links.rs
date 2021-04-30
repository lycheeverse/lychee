use lychee_lib::{ Result};
use reqwest::Url;
use lychee_lib::Input;
use std::path::PathBuf;

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // Collect all links from the following inputs
    let inputs: &[Input] = &[Input::RemoteUrl(Box::new(Url::parse("https://github.com/lycheeverse/lychee").unwrap())),
    Input::FsPath(PathBuf::from("fixtures/TEST.md"))];

    let links = lychee_lib::collector::collect_links(
        inputs,
        None,  // base_url
         false, // don't skip missing inputs
        10, // max concurrency
    ).await?;

    dbg!(links);

    Ok(())
}
