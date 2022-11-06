//! Sitemap handling

use std::pin::Pin;

use crate::Result;
use async_stream::try_stream;
use futures::Stream;
use html5ever::tendril::fmt::Slice;
use reqwest::Url;
use sitemap::reader::{SiteMapEntity, SiteMapReader};

pub struct Sitemap;

impl Sitemap {
    pub async fn urls(sitemap: Url) -> Pin<Box<dyn Stream<Item = Result<Url>>>> {
        Box::pin(try_stream! {
            let content = reqwest::get(sitemap)
                .await
                // .map_err(|e| ErrorKind::NetworkRequest(e))?
                .unwrap()
                .bytes()
                .await
                .unwrap()
                // .map_err(|e| ErrorKind::NetworkRequest(e))?
                .to_owned();

            let parser = SiteMapReader::new(content.as_bytes());
            for entity in parser {
                match entity {
                    SiteMapEntity::Url(url_entry) => {
                        if let Some(url) = url_entry.loc.get_url() {
                            println!("Found URL: {}", url);
                            yield url
                        }
                    }
                    SiteMapEntity::SiteMap(sitemap_entry) => {
                        // println!("Found sitemap: {:?}", sitemap_entry.loc);
                        if let Some(entry) = sitemap_entry.loc.get_url() {
                            let stream = Self::urls(entry).await;
                            tokio::pin!(stream);
                            for await item in stream {
                                yield item?;
                            }



                            // for await url in Sitemap::urls(entry).await {
                            //     if let Ok(url) = url {
                            //         yield url
                            //     }
                            // }
                        }
                        ()
                    }
                    SiteMapEntity::Err(e) => {
                        println!("Error: {}", e);
                        // Silently ignore errors
                        // errors.push(error);
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[tokio::test]
    async fn test_sitemap() {
        // let urls: Vec<Url> = Sitemap::urls("https:/endler.dev/sitemap.xml".parse().unwrap())
        //     .await
        //     .collect();

        // Collect stream into a vector
        let urls: Vec<Result<Url>> =
            Sitemap::urls("https://endler.dev/sitemap.xml".parse().unwrap())
                .await
                .collect()
                .await;

        assert_eq!(urls.len(), 2);
    }

    #[tokio::test]
    async fn test_sitemap_recursion() {
        let urls: Vec<Result<Url>> =
            Sitemap::urls("https://www.google.com/sitemap.xml".parse().unwrap())
                .await
                .collect()
                .await;
        assert_eq!(urls.len(), 2);
    }
}
