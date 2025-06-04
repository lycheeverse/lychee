use lychee_lib::archive::Archive;
use std::{error::Error, time::Duration};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let archive = Archive::WaybackMachine;
    let url = Url::parse("https://example.com")?;
    let result = archive
        .get_archive_snapshot(&url, Duration::from_secs(10))
        .await?;

    if let Some(replacement) = result {
        println!("Good news! {} can be replaced with {}", url, replacement);
    }

    Ok(())
}
