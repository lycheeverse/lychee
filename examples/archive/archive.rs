use lychee_lib::{Result, archive::Archive};
use std::time::Duration;
use url::Url;

#[tokio::main]
async fn main() -> Result<()> {
    let archive = Archive::WaybackMachine;
    let url = Url::parse("https://google.com/jobs.html").unwrap();
    let result = archive
        .get_snapshot(&url, Duration::from_secs(5))
        .await
        .expect("Error while fetching snapshot from the Wayback Machine");

    if let Some(replacement) = result {
        println!("Good news! {} can be replaced with {}", url, replacement);
    }

    Ok(())
}
