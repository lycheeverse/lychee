use indicatif::{ProgressBar as Bar, ProgressStyle};
use std::sync::LazyLock;

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
    /// Show detailed progress information when `true`
    detailed: bool,
}

impl Progress {
    pub(crate) fn new(initial_message: &'static str, hide_bar: bool, detailed: bool) -> Self {
        // Showing the progress bar and detailed logging is too much information
        let bar = if hide_bar || detailed {
            None
        } else {
            let bar = Bar::new_spinner().with_style(STYLE.clone());
            bar.set_length(0);
            bar.set_message(initial_message);
            Some(bar)
        };

        Progress { bar, detailed }
    }

    /// If a bar is configured it is advanced by one and optionally updated with `message`.
    /// If reporting is `detailed` `message` is printed.
    pub(crate) fn update(&self, message: Option<String>) {
        if self.detailed
            && let Some(message) = message.as_ref()
        {
            // progress is reported on stderr and NOT on stdout
            eprintln!("{message}");
        }

        self.with_bar(|bar| {
            bar.inc(1);
            if let Some(message) = message {
                bar.set_message(message);
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
