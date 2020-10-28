use crate::checker::CheckerClient;
use crate::types::Response;
use crate::types::Uri;
use anyhow::Result;
use async_channel::{Receiver, Sender};

pub struct Worker {
    requests: Receiver<Uri>,
    responses: Sender<Response>,
    checker: CheckerClient,
}

impl Worker {
    pub fn new(
        requests: Receiver<Uri>,
        responses: Sender<Response>,
        checker: CheckerClient,
    ) -> Self {
        Worker {
            requests,
            responses,
            checker,
        }
    }

    pub async fn listen(&mut self) -> Result<()> {
        loop {
            let request = self.requests.recv().await?;
            self.responses
                .send(self.checker.check(request).await)
                .await?;
        }
    }
}
