use crate::Status;
use async_trait::async_trait;
use core::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, PartialEq)]
pub(crate) enum ChainResult<T, R> {
    Next(T),
    Done(R),
}

pub(crate) type RequestChain = Chain<reqwest::Request, Status>;

pub(crate) type InnerChain<T, R> = Vec<Box<dyn Chainable<T, R> + Send>>;

#[derive(Debug)]
pub struct Chain<T, R>(Arc<Mutex<InnerChain<T, R>>>);

impl<T, R> Default for Chain<T, R> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(vec![])))
    }
}

impl<T, R> Clone for Chain<T, R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, R> Chain<T, R> {
    pub(crate) fn new(values: InnerChain<T, R>) -> Self {
        Self(Arc::new(Mutex::new(values)))
    }

    pub(crate) async fn traverse(&mut self, mut input: T) -> ChainResult<T, R> {
        use ChainResult::{Done, Next};
        for e in self.0.lock().await.iter_mut() {
            match e.chain(input).await {
                Next(r) => input = r,
                Done(r) => {
                    return Done(r);
                }
            }
        }

        Next(input)
    }

    // TODO: probably remove
    pub(crate) fn into_inner(self) -> InnerChain<T, R> {
        Arc::try_unwrap(self.0).expect("Arc still has multiple owners").into_inner()
    }
}

#[async_trait]
pub(crate) trait Chainable<T, R>: Debug {
    async fn chain(&mut self, input: T) -> ChainResult<T, R>;
}

mod test {
    use super::{
        ChainResult,
        ChainResult::{Done, Next},
        Chainable,
    };
    use async_trait::async_trait;

    #[derive(Debug)]
    struct Add(usize);

    #[derive(Debug, PartialEq, Eq)]
    struct Result(usize);

    #[async_trait]
    impl Chainable<Result, Result> for Add {
        async fn chain(&mut self, req: Result) -> ChainResult<Result, Result> {
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
        let mut chain: Chain<Result, Result> = Chain::new(vec![Box::new(Add(7)), Box::new(Add(3))]);
        let result = chain.traverse(Result(0)).await;
        assert_eq!(result, Next(Result(10)));
    }

    #[tokio::test]
    async fn early_exit_chain() {
        use super::Chain;
        let mut chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(80)), Box::new(Add(30)), Box::new(Add(1))]);
        let result = chain.traverse(Result(0)).await;
        assert_eq!(result, Done(Result(80)));
    }
}
