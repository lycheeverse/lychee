//! [Chain of responsibility pattern][pattern] implementation.
//!
//! lychee is based on a chain of responsibility, where each handler can modify
//! a request and decide if it should be passed to the next element or not.
//!
//! The chain is implemented as a vector of [`Handler`] handlers. It is
//! traversed by calling [`Chain::traverse`], which will call
//! [`Handler::chain`] on each handler in the chain consecutively.
//!
//! To add external handlers, you can implement the [`Handler`] trait and add
//! the handler to the chain.
//!
//! [pattern]: https://github.com/lpxxn/rust-design-pattern/blob/master/behavioral/chain_of_responsibility.rs
use crate::Status;
use async_trait::async_trait;
use core::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Result of a handler.
///
/// This is used to decide if the chain should continue to the next handler or
/// stop and return the result:
///
/// - If the chain should continue, the handler should return
///   [`ChainResult::Next`]. This will traverse the next handler in the chain.
/// - If the chain should stop, the handler should return [`ChainResult::Done`].
///   All subsequent chain elements are skipped and the result is returned.
#[derive(Debug, PartialEq)]
pub enum ChainResult<T, R> {
    /// Continue to the next handler in the chain.
    Next(T),
    /// Stop the chain and return the result.
    Done(R),
}

/// Request chain type
///
/// Lychee uses a chain of responsibility pattern to handle requests.
/// Each handler in the chain can modify the request and decide if it should be
/// passed to the next handler or not.
///
/// The chain is implemented as a vector of handlers. It is traversed by calling
/// `traverse` on the [`Chain`], which in turn will call [`Handler::handle`] on
/// each handler in the chain consecutively.
///
/// To add external handlers, you can implement the [`Handler`] trait and add your
/// handler to the chain.
///
/// The entire request chain takes a request as input and returns a status.
///
/// # Example
///
/// ```rust
/// use async_trait::async_trait;
/// use lychee_lib::{chain::RequestChain, ChainResult, ClientBuilder, Handler, Result, Status};
/// use reqwest::{Method, Request, Url};
///
/// // Define your own custom handler
/// #[derive(Debug)]
/// struct DummyHandler {}
///
/// #[async_trait]
/// impl Handler<Request, Status> for DummyHandler {
///     async fn handle(&mut self, mut request: Request) -> ChainResult<Request, Status> {
///         // Modify the request here
///         // After that, continue to the next handler
///         ChainResult::Next(request)
///     }
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     // Build a custom request chain with our dummy handler
///     let chain = RequestChain::new(vec![Box::new(DummyHandler {})]);
///
///     let client = ClientBuilder::builder()
///         .plugin_request_chain(chain)
///         .build()
///         .client()?;
///
///     let result = client.check("https://wikipedia.org").await;
///     println!("{:?}", result);
///
///     Ok(())
/// }
/// ```
pub type RequestChain = Chain<reqwest::Request, Status>;

/// Inner chain type.
///
/// This holds all handlers, which were chained together.
/// Handlers are traversed in order.
///
/// Each handler needs to implement the `Handler` trait and be `Send`, because
/// the chain is traversed concurrently and the handlers can be sent between
/// threads.
pub(crate) type InnerChain<T, R> = Vec<Box<dyn Handler<T, R> + Send>>;

/// The outer chain type.
///
/// This is a wrapper around the inner chain type and allows for
/// concurrent access to the chain.
#[derive(Debug)]
pub struct Chain<T, R>(Arc<Mutex<InnerChain<T, R>>>);

impl<T, R> Default for Chain<T, R> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(InnerChain::default())))
    }
}

impl<T, R> Clone for Chain<T, R> {
    fn clone(&self) -> Self {
        // Cloning the chain is a cheap operation, because the inner chain is
        // wrapped in an `Arc` and `Mutex`.
        Self(self.0.clone())
    }
}

impl<T, R> Chain<T, R> {
    /// Create a new chain from a vector of chainable handlers
    #[must_use]
    pub fn new(values: InnerChain<T, R>) -> Self {
        Self(Arc::new(Mutex::new(values)))
    }

    /// Traverse the chain with the given input.
    ///
    /// This will call `chain` on each handler in the chain and return
    /// the result. If a handler returns `ChainResult::Done`, the chain
    /// will stop and return.
    ///
    /// If no handler returns `ChainResult::Done`, the chain will return
    /// `ChainResult::Next` with the input.
    pub(crate) async fn traverse(&self, mut input: T) -> ChainResult<T, R> {
        use ChainResult::{Done, Next};
        for e in self.0.lock().await.iter_mut() {
            match e.handle(input).await {
                Next(r) => input = r,
                Done(r) => {
                    return Done(r);
                }
            }
        }

        Next(input)
    }
}

/// Handler trait for implementing request handlers
///
/// This trait needs to be implemented by all chainable handlers.
/// It is the only requirement to handle requests in lychee.
///
/// It takes an input request and returns a [`ChainResult`], which can be either
/// [`ChainResult::Next`] to continue to the next handler or
/// [`ChainResult::Done`] to stop the chain.
///
/// The request can be modified by the handler before it is passed to the next
/// handler. This allows for modifying the request, such as adding headers or
/// changing the URL (e.g. for remapping or filtering).
#[async_trait]
pub trait Handler<T, R>: Debug {
    /// Given an input request, return a [`ChainResult`] to continue or stop the
    /// chain.
    ///
    /// The input request can be modified by the handler before it is passed to
    /// the next handler.
    ///
    /// # Example
    ///
    /// ```
    /// use lychee_lib::{Handler, ChainResult, Status};
    /// use reqwest::Request;
    /// use async_trait::async_trait;
    ///
    /// #[derive(Debug)]
    /// struct AddHeader;
    ///
    /// #[async_trait]
    /// impl Handler<Request, Status> for AddHeader {
    ///    async fn handle(&mut self, mut request: Request) -> ChainResult<Request, Status> {
    ///      // You can modify the request however you like here
    ///      request.headers_mut().append("X-Header", "value".parse().unwrap());
    ///
    ///      // Pass the request to the next handler
    ///      ChainResult::Next(request)
    ///   }
    /// }
    /// ```
    async fn handle(&mut self, input: T) -> ChainResult<T, R>;
}

/// Client request chains
///
/// This struct holds all request chains.
///
/// Usually, this is used to hold the default request chain and the external
/// plugin request chain.
#[derive(Debug)]
pub(crate) struct ClientRequestChains<'a> {
    chains: Vec<&'a RequestChain>,
}

impl<'a> ClientRequestChains<'a> {
    /// Create a new chain of request chains.
    pub(crate) const fn new(chains: Vec<&'a RequestChain>) -> Self {
        Self { chains }
    }

    /// Traverse all request chains and resolve to a status.
    pub(crate) async fn traverse(&self, mut input: reqwest::Request) -> Status {
        use ChainResult::{Done, Next};

        for e in &self.chains {
            match e.traverse(input).await {
                Next(r) => input = r,
                Done(r) => {
                    return r;
                }
            }
        }

        // Consider the request to be excluded if no chain element has converted
        // it to a `ChainResult::Done`
        Status::Excluded
    }
}

mod test {
    use super::{
        ChainResult,
        ChainResult::{Done, Next},
        Handler,
    };
    use async_trait::async_trait;

    #[allow(dead_code)] // work-around
    #[derive(Debug)]
    struct Add(usize);

    #[derive(Debug, PartialEq, Eq)]
    struct Result(usize);

    #[async_trait]
    impl Handler<Result, Result> for Add {
        async fn handle(&mut self, req: Result) -> ChainResult<Result, Result> {
            let added = req.0 + self.0;
            if added > 100 {
                Done(Result(req.0))
            } else {
                Next(Result(added))
            }
        }
    }

    #[tokio::test]
    async fn simple_chain() {
        use super::Chain;
        let chain: Chain<Result, Result> = Chain::new(vec![Box::new(Add(7)), Box::new(Add(3))]);
        let result = chain.traverse(Result(0)).await;
        assert_eq!(result, Next(Result(10)));
    }

    #[tokio::test]
    async fn early_exit_chain() {
        use super::Chain;
        let chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(80)), Box::new(Add(30)), Box::new(Add(1))]);
        let result = chain.traverse(Result(0)).await;
        assert_eq!(result, Done(Result(80)));
    }
}
