use console::Style;
use lychee_lib::{ResponseBody, Status};
use once_cell::sync::Lazy;

static GREEN: Lazy<Style> = Lazy::new(|| Style::new().green().bright());
static DIM: Lazy<Style> = Lazy::new(|| Style::new().dim());
static NORMAL: Lazy<Style> = Lazy::new(Style::new);
static YELLOW: Lazy<Style> = Lazy::new(|| Style::new().yellow().bright());
static RED: Lazy<Style> = Lazy::new(|| Style::new().red().bright());

pub(crate) fn color_response(response: &ResponseBody) -> String {
    let out = match response.status {
        Status::Ok(_) => GREEN.apply_to(response),
        Status::Excluded | Status::Unsupported(_) => DIM.apply_to(response),
        Status::Redirected(_) => NORMAL.apply_to(response),
        Status::UnknownStatusCode(_) | Status::Timeout(_) => YELLOW.apply_to(response),
        Status::Error(_) => RED.apply_to(response),
    };
    out.to_string()
}
