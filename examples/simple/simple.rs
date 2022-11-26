use lychee_lib::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let response = lychee_lib::check("https://github.com/lycheeverse/lychee").await?;
    dbg!("{}", response);
    Ok(())
}
