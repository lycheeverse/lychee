use core::fmt::Debug;

use crate::Status;

#[derive(Debug, PartialEq)]
pub(crate) enum ChainResult<T, R> {
    Next(T),
    Done(R),
}

pub(crate) type RequestChain = Chain<reqwest::Request, Status>;

#[derive(Debug)]
pub struct Chain<T, R>(Vec<Box<dyn Chainable<T, R> + Send>>);

impl<T, R> Default for Chain<T, R> {
    fn default() -> Self {
        Self(vec![])
    }
}

impl<T, R> Chain<T, R> {
    pub(crate) fn new(values: Vec<Box<dyn Chainable<T, R> + Send>>) -> Self {
        Self(values)
    }

    pub(crate) fn append(&mut self, other: &mut Chain<T, R>) {
        self.0.append(&mut other.0);
    }

    pub(crate) fn traverse(&mut self, mut input: T) -> ChainResult<T, R> {
        use ChainResult::*;
        for e in self.0.iter_mut() {
            match e.chain(input) {
                Next(r) => input = r,
                Done(r) => {
                    return Done(r);
                }
            }
        }

        Next(input)
    }

    pub(crate) fn push(&mut self, value: Box<dyn Chainable<T, R> + Send>) {
        self.0.push(value);
    }
}

pub(crate) trait Chainable<T, R>: Debug {
    fn chain(&mut self, input: T) -> ChainResult<T, R>;
}

mod test {
    use super::{ChainResult, ChainResult::*, Chainable};

    #[derive(Debug)]
    struct Add(usize);

    #[derive(Debug, PartialEq, Eq)]
    struct Result(usize);

    impl Chainable<Result, Result> for Add {
        fn chain(&mut self, req: Result) -> ChainResult<Result, Result> {
            let added = req.0 + self.0;
            if added > 100 {
                Done(Result(req.0))
            } else {
                Next(Result(added))
            }
        }
    }

    #[test]
    fn simple_chain() {
        use super::Chain;
        let mut chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(7)), Box::new(Add(3))]);
        let result = chain.traverse(Result(0));
        assert_eq!(result, Next(Result(10)));
    }

    #[test]
    fn early_exit_chain() {
        use super::Chain;
        let mut chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(80)), Box::new(Add(30)), Box::new(Add(1))]);
        let result = chain.traverse(Result(0));
        assert_eq!(result, Done(Result(80)));
    }
}
