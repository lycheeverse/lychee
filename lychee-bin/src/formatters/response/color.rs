use crate::formatters::color_response;

use super::ResponseFormatter;

use lychee_lib::{Response, Result};

/// A formatter which colors the response as long as that is supported by the
/// environment (and not overwritten with `NO_COLOR=1`)
pub(crate) struct Color;

impl Color {
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl ResponseFormatter for Color {
    fn write_response(&self, response: &Response) -> Result<String> {
        Ok(color_response(&response.1))
    }
}
