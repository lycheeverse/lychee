use lychee_lib::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let response =
        lychee_lib::check("https://github.com/lycheeverse/lychee".try_into().unwrap()).await?;
    dbg!("{}", response);
    Ok(())
}
