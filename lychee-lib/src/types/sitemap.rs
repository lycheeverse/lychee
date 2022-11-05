//! Sitemap handling

use crate::{ErrorKind, Result};
use reqwest::Url;
use sitemap::reader::{SiteMapEntity, SiteMapReader};
use std::{
    fs::File,
    io::{BufReader, Read},
};

pub struct Sitemap {}

impl Sitemap {
    /// Parse a sitemap from a URL
    async fn fetch(url: &Url) -> Result<SiteMapReader<impl Read>> {
        let response = reqwest::get(url.clone())
            .await
            .map_err(|e| ErrorKind::NetworkRequest(e))?;
        let body = response.bytes().await.map_err(ErrorKind::NetworkRequest)?;
        let reader = BufReader::new(body.to_owned());
        Ok(SiteMapReader::new(reader))
    }

    pub fn urls(sitemap: Url) -> Vec<Url> {
        let mut urls = Vec::new();
        let mut sitemaps = Vec::new();
        let mut errors = Vec::new();
        let file = File::open("sitemap.xml").expect("Unable to open file.");
        let parser = SiteMapReader::new(file);
        for entity in parser {
            match entity {
                SiteMapEntity::Url(url_entry) => {
                    if let Some(url) = url_entry.loc.get_url() {
                        urls.push(url);
                    }
                }
                SiteMapEntity::SiteMap(sitemap_entry) => {
                    sitemaps.push(sitemap_entry);
                }
                SiteMapEntity::Err(error) => {
                    errors.push(error);
                }
            }
        }
        // println!("urls = {:?}", urls);
        // println!("sitemaps = {:?}", sitemaps);
        // println!("errors = {:?}", errors);

        urls
    }
}
