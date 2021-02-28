use client::Client;
use deadpool::unmanaged::Pool;
use tokio::sync::mpsc;

use crate::{client, types};

pub struct ClientPool {
    tx: mpsc::Sender<types::Response>,
    rx: mpsc::Receiver<types::Request>,
    pool: deadpool::unmanaged::Pool<client::Client>,
}

impl ClientPool {
    pub fn new(
        tx: mpsc::Sender<types::Response>,
        rx: mpsc::Receiver<types::Request>,
        clients: Vec<Client>,
    ) -> Self {
        let pool = Pool::from(clients);
        ClientPool { tx, rx, pool }
    }

    pub async fn listen(&mut self) {
        while let Some(uri) = self.rx.recv().await {
            let client = self.pool.get().await;
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let resp = client.check(uri).await.expect("Invalid URI");
                tx.send(resp)
                    .await
                    .expect("Cannot send response to channel");
            });
        }
    }
}
