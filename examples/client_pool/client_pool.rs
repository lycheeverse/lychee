use lychee_lib::{ClientBuilder, Input, Request, Result, Uri};
use std::convert::TryFrom;
use tokio::sync::mpsc;

const CONCURRENT_REQUESTS: usize = 4;

#[tokio::main]
#[allow(clippy::trivial_regex)]
async fn main() -> Result<()> {
    // These channels are used to send requests and receive responses to and
    // from the lychee client pool
    let (send_req, mut recv_req) = mpsc::channel(CONCURRENT_REQUESTS);
    let (send_resp, mut recv_resp) = mpsc::channel(CONCURRENT_REQUESTS);

    // Add as many requests as you like
    let requests = vec![Request::new(
        Uri::try_from("https://example.org")?,
        Input::Stdin,
    )];

    // Send requests to pool
    tokio::spawn(async move {
        for request in requests {
            println!("Sending {}", request);
            send_req.send(request).await.unwrap();
        }
    });

    // Create a default lychee client
    let client = ClientBuilder::default().client()?;

    // Handle requests in a client pool
    tokio::spawn(async move {
        while let Some(req) = recv_req.recv().await {
            // Client::check() may fail only because Request::try_from() may fail
            // here request is already Request, so it never fails
            let resp = client.check(req).await.unwrap();
            send_resp
                .send(resp)
                .await
                .expect("Cannot send response to channel");
        }
    });

    // Finally, listen to incoming responses from lychee
    while let Some(response) = recv_resp.recv().await {
        println!("{}", response);
    }

    Ok(())
}
