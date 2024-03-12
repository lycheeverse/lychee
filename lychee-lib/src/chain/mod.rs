use core::fmt::Debug;
use headers::authorization::Credentials;
use http::header::AUTHORIZATION;
use reqwest::Request;

use crate::BasicAuthCredentials;

pub(crate) type RequestChain = Chain<reqwest::Request>;

#[derive(Debug)]
pub struct Chain<T>(Vec<Box<dyn Chainable<T> + Send>>);

impl<T> Chain<T> {
    pub(crate) fn new(values: Vec<Box<dyn Chainable<T> + Send>>) -> Self {
        Self(values)
    }

    pub(crate) fn traverse(&mut self, mut input: T) -> T {
        for e in self.0.iter_mut() {
            input = e.handle(input)
        }

        input
    }

    pub(crate) fn push(&mut self, value: Box<dyn Chainable<T> + Send>) {
        self.0.push(value);
    }
}

pub(crate) trait Chainable<T>: Debug {
    fn handle(&mut self, input: T) -> T;
}

#[derive(Debug)]
pub(crate) struct BasicAuth {
    credentials: BasicAuthCredentials,
}

impl BasicAuth {
    pub(crate) fn new(credentials: BasicAuthCredentials) -> Self {
        Self { credentials }
    }
}

impl Chainable<Request> for BasicAuth {
    fn handle(&mut self, mut request: Request) -> Request {
        request.headers_mut().append(
            AUTHORIZATION,
            self.credentials.to_authorization().0.encode(),
        );
        request
    }
}

mod test {
    use super::Chainable;

    #[derive(Debug)]
    struct Add(i64);

    #[derive(Debug)]
    struct Request(i64);

    impl Chainable<Request> for Add {
        fn handle(&mut self, req: Request) -> Request {
            Request(req.0 + self.0)
        }
    }

    #[test]
    fn example_chain() {
        use super::Chain;
        let mut chain: Chain<Request> = Chain::new(vec![Box::new(Add(10)), Box::new(Add(-3))]);
        let result = chain.traverse(Request(0));
        assert_eq!(result.0, 7);
    }
}
