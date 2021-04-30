use lychee_lib::{ClientBuilder, ClientPool, Result, Request, Uri, Input};
use tokio::sync::mpsc;
use std::convert::TryFrom;

const CONCURRENT_REQUESTS: usize = 4;

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // These channels are used to send requests and receive responses to and
    // from the lychee client pool
    let (send_req, recv_req) = mpsc::channel(CONCURRENT_REQUESTS);
    let (send_resp, mut recv_resp) = mpsc::channel(CONCURRENT_REQUESTS);

    // Add as many requests as you like
    let requests = vec![Request::new(
        Uri::try_from("https://example.org")?,
        Input::Stdin,
    )];

    // Send requests to pool
    tokio::spawn(async move {
        for request in requests {
            println!("Sending {}",request);
            send_req.send(request).await.unwrap();
        };
    });

    // Create a default lychee client
    let client = ClientBuilder::default().client()?;

    // Create a pool with four lychee clients
    let clients = vec![client; CONCURRENT_REQUESTS];
    let mut clients = ClientPool::new(send_resp, recv_req, clients);

    // Handle requests in a client pool
    tokio::spawn(async move {
        clients.listen().await;
    });

    // Finally, listen to incoming responses from lychee
    while let Some(response) = recv_resp.recv().await {
        println!("{}",response);
    }

    Ok(())
}
