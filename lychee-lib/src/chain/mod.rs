use crate::Status;
use async_trait::async_trait;
use core::fmt::Debug;
use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq)]
pub(crate) enum ChainResult<T, R> {
    Next(T),
    Done(R),
}

pub(crate) type RequestChain = Chain<reqwest::Request, Status>;

#[derive(Debug)]
pub struct Chain<T, R>(Arc<Mutex<Vec<Box<dyn Chainable<T, R> + Send>>>>);

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
    pub(crate) fn new(values: Vec<Box<dyn Chainable<T, R> + Send>>) -> Self {
        Self(Arc::new(Mutex::new(values)))
    }

    pub(crate) fn append(&mut self, other: &Chain<T, R>) {
        self.0.lock().unwrap().append(&mut other.0.lock().unwrap());
    }

    pub(crate) async fn traverse(&mut self, mut input: T) -> ChainResult<T, R> {
        use ChainResult::{Done, Next};
        for e in self.0.lock().unwrap().iter_mut() {
            match e.chain(input).await {
                Next(r) => input = r,
                Done(r) => {
                    return Done(r);
                }
            }
        }

        Next(input)
    }

    pub(crate) fn push(&mut self, value: Box<dyn Chainable<T, R> + Send>) {
        self.0.lock().unwrap().push(value);
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
