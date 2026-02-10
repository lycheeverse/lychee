use indicatif::{ProgressBar as Bar, ProgressStyle};
use log::Level;
use lychee_lib::ResponseBody;
use std::sync::{Arc, LazyLock};

use crate::{
    formatters::{get_progress_formatter, response::ResponseFormatter},
    options::OutputMode,
};

#[derive(Clone)]
struct ProgressConfig {
    template: &'static str,
    progress_chars: &'static str,
}

const CONFIG: ProgressConfig = ProgressConfig {
    template: "{pos}/{len:.238} {bar:.162/238} {wide_msg}",
    progress_chars: "━ ━",
};

static STYLE: LazyLock<ProgressStyle> = LazyLock::new(|| {
    ProgressStyle::with_template(CONFIG.template)
        .expect("Valid progress bar")
        .progress_chars(CONFIG.progress_chars)
});

#[derive(Clone)]
/// Report link check progress on stderr.
pub(crate) struct Progress {
    /// Optional progress bar to visualize progress
    bar: Option<Bar>,
    log_level: Level,
    formatter: Arc<Box<dyn ResponseFormatter>>,
}

impl Progress {
    pub(crate) fn new(
        initial_message: &'static str,
        hide_bar: bool,
        log_level: Level,
        mode: &OutputMode,
    ) -> Self {
        let detailed = log_level >= Level::Info; // hide bar with detailed logging
        let bar = if hide_bar || detailed {
            None
        } else {
            let bar = Bar::new_spinner().with_style(STYLE.clone());
            bar.set_length(0);
            bar.set_message(initial_message);
            Some(bar)
        };

        Progress {
            bar,
            log_level,
            formatter: Arc::new(get_progress_formatter(mode)),
        }
    }

    /// If a bar is configured it is advanced by one and optionally updated with `response`.
    /// Progress output depends on the provided log level.
    pub(crate) fn update(&self, response: Option<&ResponseBody>) {
        let response_message = response.map(|r| (r, self.formatter.format_response(r)));
        self.print_progress(response_message.as_ref());
        self.with_bar(|bar| {
            bar.inc(1);
            if let Some((_, message)) = response_message {
                bar.set_message(message);
            }
        });
    }

    fn print_progress(&self, response_message: Option<&(&ResponseBody, String)>) {
        if self.log_level < Level::Info {
            return;
        }

        let Some((response, message)) = response_message else {
            return;
        };

        let should_show_success = self.log_level > Level::Info;
        let is_success = response.status.is_success();

        if !is_success || should_show_success {
            eprintln!("{message}");
        }
    }

    pub(crate) fn set_length(&self, n: u64) {
        self.with_bar(|b| b.set_length(n));
    }

    pub(crate) fn inc_length(&self, n: u64) {
        self.with_bar(|b| b.inc_length(n));
    }

    pub(crate) fn finish(&self, message: &'static str) {
        self.with_bar(|b| b.finish_with_message(message));
    }

    fn with_bar<F>(&self, action: F)
    where
        F: FnOnce(&Bar),
    {
        if let Some(bar) = &self.bar {
            action(bar);
        }
    }
}
