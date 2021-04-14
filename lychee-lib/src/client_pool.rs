use client::Client;
use deadpool::unmanaged::Pool;
use tokio::sync::mpsc;

use crate::{client, types};

#[allow(missing_debug_implementations)]
pub struct ClientPool {
    tx: mpsc::Sender<types::Response>,
    rx: mpsc::Receiver<types::Request>,
    pool: deadpool::unmanaged::Pool<client::Client>,
}

impl ClientPool {
    #[must_use]
    pub fn new(
        tx: mpsc::Sender<types::Response>,
        rx: mpsc::Receiver<types::Request>,
        clients: Vec<Client>,
    ) -> Self {
        let pool = Pool::from(clients);
        ClientPool { tx, rx, pool }
    }

    #[allow(clippy::missing_panics_doc)]
    pub async fn listen(&mut self) {
        while let Some(req) = self.rx.recv().await {
            let client = self.pool.get().await;
            let tx = self.tx.clone();
            tokio::spawn(async move {
                // Client::check() may fail only because Request::try_from() may fail
                // here request is already Request, so it never fails
                let resp = client.check(req).await.unwrap();
                tx.send(resp)
                    .await
                    .expect("Cannot send response to channel");
            });
        }
    }
}
