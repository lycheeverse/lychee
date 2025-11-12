use indicatif::{ProgressBar as Bar, ProgressStyle};
use lychee_lib::{Response, Result};
use std::{io::Write, sync::LazyLock};

use crate::formatters::response::ResponseFormatter;

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
/// Report progress to the CLI.
pub(crate) struct Progress {
    bar: Option<Bar>,
    detailed: bool,
}

impl Progress {
    pub(crate) fn new(initial_message: &'static str, hidden: bool, detailed: bool) -> Self {
        // Showing the progress bar and detailed logging is too much information
        let bar = if hidden || detailed {
            None
        } else {
            let bar = Bar::new_spinner().with_style(STYLE.clone());
            bar.set_length(0);
            bar.set_message(initial_message);
            Some(bar)
        };

        Progress { bar, detailed }
    }

    pub(crate) fn show(
        &self,
        output: &mut dyn Write,
        response: &Response,
        formatter: &dyn ResponseFormatter,
    ) -> Result<()> {
        let out = if self.detailed {
            formatter.format_detailed_response(response.body())
        } else {
            formatter.format_response(response.body())
        };

        if self.detailed {
            writeln!(output, "{}", &out)?;
        }

        self.update(Some(out));
        Ok(())
    }

    pub(crate) fn update(&self, message: Option<String>) {
        self.with_bar(|bar| {
            bar.inc(1);
            if let Some(msg) = message {
                bar.set_message(msg);
            }
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{formatters::get_response_formatter, options};
    use log::info;
    use lychee_lib::{CacheStatus, ResolvedInputSource, Status, Uri};

    #[test]
    fn test_skip_cached_responses_in_progress_output() {
        let mut buf = Vec::new();
        let response = Response::new(
            Uri::try_from("http://127.0.0.1").unwrap(),
            Status::Cached(CacheStatus::Ok(200)),
            ResolvedInputSource::Stdin,
        );
        let formatter = get_response_formatter(&options::OutputMode::Plain);
        let progress = Progress::new("", false, false);
        progress
            .show(&mut buf, &response, formatter.as_ref())
            .unwrap();

        info!("{:?}", String::from_utf8_lossy(&buf));
        assert!(buf.is_empty());
    }

    #[test]
    fn test_show_cached_responses_in_progress_debug_output() {
        let mut buf = Vec::new();
        let response = Response::new(
            Uri::try_from("http://127.0.0.1").unwrap(),
            Status::Cached(CacheStatus::Ok(200)),
            ResolvedInputSource::Stdin,
        );

        let progress = Progress::new("", false, true);
        let formatter = get_response_formatter(&options::OutputMode::Plain);
        progress
            .show(&mut buf, &response, formatter.as_ref())
            .unwrap();

        assert!(!buf.is_empty());
        let buf = String::from_utf8_lossy(&buf);
        assert_eq!(buf, "[200] http://127.0.0.1/ | OK (cached)\n");
    }
}
