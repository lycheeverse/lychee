use client::Client;
use deadpool::unmanaged::Pool;
use tokio::sync::mpsc;

use crate::{client, types};

#[allow(missing_debug_implementations)]
/// Manages a channel for incoming requests
/// and a pool of lychee clients to handle them
///
/// Note: Although `reqwest` has its own pool,
/// it only works for connections to the same host, so
/// a single client can still be blocked until a request is done.
pub struct ClientPool {
    tx: mpsc::Sender<types::Response>,
    rx: mpsc::Receiver<types::Request>,
    pool: deadpool::unmanaged::Pool<client::Client>,
}

impl ClientPool {
    #[must_use]
    /// Creates a new client pool
    pub fn new(
        tx: mpsc::Sender<types::Response>,
        rx: mpsc::Receiver<types::Request>,
        clients: Vec<Client>,
    ) -> Self {
        let pool = Pool::from(clients);
        ClientPool { tx, rx, pool }
    }

    #[allow(clippy::missing_panics_doc)]
    /// Start listening for incoming requests and send each of them
    /// asynchronously to a client from the pool
    pub async fn listen(&mut self) {
        while let Some(req) = self.rx.recv().await {
            let client = self.pool.get().await.unwrap();
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
