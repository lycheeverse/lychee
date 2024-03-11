use headers::authorization::Credentials;
use http::header::AUTHORIZATION;
use reqwest::Request;

use crate::BasicAuthCredentials;

pub(crate) type Chain<T> = Vec<Box<dyn Chainable<T> + Send>>;

pub(crate) fn traverse<T>(chain: Chain<T>, mut input: T) -> T {
    for mut e in chain {
        input = e.handle(input)
    }

    input
}

pub(crate) trait Chainable<T> {
    fn handle(&mut self, input: T) -> T;
}

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

    struct Add(i64);

    struct Request(i64);

    impl Chainable<Request> for Add {
        fn handle(&mut self, req: Request) -> Request {
            Request(req.0 + self.0)
        }
    }

    #[test]
    fn example_chain() {
        let chain: crate::chain::Chain<Request> = vec![Box::new(Add(10)), Box::new(Add(-3))];
        let result = crate::chain::traverse(chain, Request(0));
        assert_eq!(result.0, 7);
    }
}
