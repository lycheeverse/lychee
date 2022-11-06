//! Sitemap handling

use html5ever::tendril::fmt::Slice;
use reqwest::Url;
use sitemap::reader::{SiteMapEntity, SiteMapReader};

pub struct Sitemap {}

impl Sitemap {
    async fn fetch(url: &Url) -> Result<bytes::Bytes, reqwest::Error> {
        Ok(reqwest::get(url.as_str())
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_owned())
    }
    pub async fn urls(root_sitemap: Url) -> Vec<Url> {
        let mut urls = Vec::new();
        let mut sitemaps: Vec<Url> = vec![root_sitemap];

        if let Some(sitemap) = sitemaps.pop() {
            let parser = SiteMapReader::new(content.as_bytes());
            for entity in parser {
                match entity {
                    SiteMapEntity::Url(url_entry) => {
                        if let Some(url) = url_entry.loc.get_url() {
                            urls.push(url);
                        }
                    }
                    SiteMapEntity::SiteMap(sitemap_entry) => {
                        if let Some(url) = sitemap_entry.loc.get_url() {
                            sitemaps.push(url);
                        }
                    }
                    SiteMapEntity::Err(error) => {
                        // Silently ignore errors
                        // errors.push(error);
                    }
                }
            }
        }
        // println!("urls = {:?}", urls);
        // println!("sitemaps = {:?}", sitemaps);
        // println!("errors = {:?}", errors);

        urls
    }
}
