use lychee_lib::Result;
use lychee_lib::extract::Extractor;
use lychee_lib::{FileType, InputContent};
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let input = fs::read_to_string("fixtures/elvis.html").unwrap();
    let extractor = Extractor::default();
    let links = extractor.extract(&InputContent::from_string(&input, FileType::Html));
    println!("{links:#?}");

    Ok(())
}
