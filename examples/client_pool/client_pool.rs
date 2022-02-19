use lychee_lib::{ClientBuilder, Request, Result};
use std::convert::TryFrom;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const CONCURRENT_REQUESTS: usize = 4;

#[tokio::main]
async fn main() -> Result<()> {
    // These channels are used to send requests and receive responses to and
    // from lychee
    let (send_req, recv_req) = mpsc::channel(CONCURRENT_REQUESTS);
    let (send_resp, mut recv_resp) = mpsc::channel(CONCURRENT_REQUESTS);

    // Add as many requests as you like
    let requests = vec![Request::try_from("https://example.com")?];

    // Queue requests
    tokio::spawn(async move {
        for request in requests {
            send_req.send(request).await.unwrap();
        }
    });

    // Create a default lychee client
    let client = ClientBuilder::default().client()?;

    // Start receiving requests
    // Requests get streamed into the client and run concurrently
    tokio::spawn(async move {
        futures::StreamExt::for_each_concurrent(
            ReceiverStream::new(recv_req),
            CONCURRENT_REQUESTS,
            |req| async {
                let resp = client.check(req).await.unwrap();
                send_resp.send(resp).await.unwrap();
            },
        )
        .await;
    });

    // Finally, listen to incoming responses from lychee
    while let Some(response) = recv_resp.recv().await {
        println!("{response}");
    }

    Ok(())
}
