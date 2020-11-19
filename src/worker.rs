use crate::types::Uri;
use crate::{client::Client, types::Response};
use anyhow::Result;
use async_channel::{Receiver, Sender};

pub struct Worker {
    requests: Receiver<Uri>,
    responses: Sender<Response>,
    client: Client,
}

impl Worker {
    pub fn new(requests: Receiver<Uri>, responses: Sender<Response>, client: Client) -> Self {
        Worker {
            requests,
            responses,
            client,
        }
    }

    pub async fn listen(&mut self) -> Result<()> {
        loop {
            let request = self.requests.recv().await?;
            self.responses
                .send(self.client.check(request).await)
                .await?;
        }
    }
}
