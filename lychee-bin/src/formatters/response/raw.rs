use super::ResponseFormatter;

use lychee_lib::{Response, Result};

/// Formatter which retruns an unmodified response status
pub(crate) struct Raw;

impl Raw {
    pub(crate) const fn new() -> Self {
        Raw {}
    }
}

impl ResponseFormatter for Raw {
    fn write_response(&self, response: &Response) -> Result<String> {
        Ok(response.1.to_string())
    }
}
