use core::fmt::Debug;

#[derive(Debug, PartialEq)]
pub(crate) enum ChainResult<T, R> {
    Chained(T),
    EarlyExit(R),
}

pub(crate) type RequestChain = Chain<reqwest::Request, crate::Response>;

#[derive(Debug)]
pub struct Chain<T, R>(Vec<Box<dyn Chainable<T, R> + Send>>);

impl<T, R> Chain<T, R> {
    pub(crate) fn new(values: Vec<Box<dyn Chainable<T, R> + Send>>) -> Self {
        Self(values)
    }

    pub(crate) fn traverse(&mut self, mut input: T) -> ChainResult<T, R> {
        use ChainResult::*;
        for e in self.0.iter_mut() {
            match e.handle(input) {
                Chained(r) => input = r,
                EarlyExit(r) => {
                    return EarlyExit(r);
                }
            }
        }

        Chained(input)
    }

    pub(crate) fn push(&mut self, value: Box<dyn Chainable<T, R> + Send>) {
        self.0.push(value);
    }
}

pub(crate) trait Chainable<T, R>: Debug {
    fn handle(&mut self, input: T) -> ChainResult<T, R>;
}

mod test {
    use super::{ChainResult, ChainResult::*, Chainable};

    #[derive(Debug)]
    struct Add(i64);

    #[derive(Debug, PartialEq, Eq)]
    struct Result(i64);

    impl Chainable<Result, Result> for Add {
        fn handle(&mut self, req: Result) -> ChainResult<Result, Result> {
            let added = req.0 + self.0;
            if added > 100 {
                EarlyExit(Result(req.0))
            } else {
                Chained(Result(added))
            }
        }
    }

    #[test]
    fn simple_chain() {
        use super::Chain;
        let mut chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(10)), Box::new(Add(-3))]);
        let result = chain.traverse(Result(0));
        assert_eq!(result, Chained(Result(7)));
    }

    #[test]
    fn early_exit_chain() {
        use super::Chain;
        let mut chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(80)), Box::new(Add(30)), Box::new(Add(1))]);
        let result = chain.traverse(Result(0));
        assert_eq!(result, EarlyExit(Result(80)));
    }
}
