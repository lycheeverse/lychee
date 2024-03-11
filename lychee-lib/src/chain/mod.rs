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
