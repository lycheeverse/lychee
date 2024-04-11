use crate::{
    chain::{ChainResult, Chainable},
    retry::RetryExt,
    Status,
};
use async_trait::async_trait;
use http::StatusCode;
use reqwest::Request;
use std::{collections::HashSet, time::Duration};

#[derive(Debug, Clone)]
pub(crate) struct Checker {
    retry_wait_time: Duration,
    max_retries: u64,
    reqwest_client: reqwest::Client,
    accepted: Option<HashSet<StatusCode>>,
}

impl Checker {
    pub(crate) const fn new(
        retry_wait_time: Duration,
        max_retries: u64,
        reqwest_client: reqwest::Client,
        accepted: Option<HashSet<StatusCode>>,
    ) -> Self {
        Self {
            retry_wait_time,
            max_retries,
            reqwest_client,
            accepted,
        }
    }

    /// Retry requests up to `max_retries` times
    /// with an exponential backoff.
    pub(crate) async fn retry_request(&self, request: Request) -> Status {
        let mut retries: u64 = 0;
        let mut wait_time = self.retry_wait_time;

        let mut status = self.check_default(clone_unwrap(&request)).await;
        while retries < self.max_retries {
            if status.is_success() || !status.should_retry() {
                return status;
            }
            retries += 1;
            tokio::time::sleep(wait_time).await;
            wait_time = wait_time.saturating_mul(2);
            status = self.check_default(clone_unwrap(&request)).await;
        }
        status
    }

    /// Check a URI using [reqwest](https://github.com/seanmonstar/reqwest).
    async fn check_default(&self, request: Request) -> Status {
        match self.reqwest_client.execute(request).await {
            Ok(ref response) => Status::new(response, self.accepted.clone()),
            Err(e) => e.into(),
        }
    }
}

/// SAFETY: unwrapping the `try_clone` of `reqwest::Request` is safe because a request only fails to be cloned when `body` of `Request` is a stream
/// and `body` cannot be a stream as long as the `stream` feature is disabled.
fn clone_unwrap(request: &Request) -> Request {
    request.try_clone().unwrap()
}

#[async_trait]
impl Chainable<Request, Status> for Checker {
    async fn chain(&mut self, input: Request) -> ChainResult<Request, Status> {
        ChainResult::Done(self.retry_request(input).await)
    }
}
