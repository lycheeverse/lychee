use async_trait::async_trait;
use lychee_lib::{ChainResult, ClientBuilder, Handler, Result, Status, chain::RequestChain};
use reqwest::{Method, Request};

#[derive(Debug)]
struct MyHandler {}

#[async_trait]
impl Handler<Request, Status> for MyHandler {
    async fn handle(&mut self, mut request: Request) -> ChainResult<Request, Status> {
        // Handle special case of some website (fictional example)
        if request.url().domain() == Some("wikipedia.org") && request.url().path() == "/home" {
            request.url_mut().set_path("/foo-bar");
            *request.method_mut() = Method::PUT;
        }

        ChainResult::Next(request)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let chain = RequestChain::new(vec![Box::new(MyHandler {})]);

    let client = ClientBuilder::builder()
        .plugin_request_chain(chain)
        .build()
        .client()?;

    let result = client.check("https://wikipedia.org/home").await;
    println!("{:?}", result);

    Ok(())
}
