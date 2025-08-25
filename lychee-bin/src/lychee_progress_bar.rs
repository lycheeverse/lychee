use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct LycheeProgressBar {
    bar: ProgressBar,
}

impl LycheeProgressBar {
    pub(crate) fn new(initial_message: &'static str) -> Self {
        let bar = ProgressBar::new_spinner().with_style(
            ProgressStyle::with_template(
                "{spinner:.162} {pos}/{len:.238} {bar:.162/238} {wide_msg}",
            )
            .expect("Valid progress bar")
            .progress_chars("━ ━"),
        );
        bar.set_length(0);
        bar.set_message(initial_message);
        // report status _at least_ every 500ms
        bar.enable_steady_tick(Duration::from_millis(500));
        LycheeProgressBar { bar }
    }

    pub(crate) fn update(&self, out: String, print: bool) {
        self.inc();
        self.bar.set_message(out.clone());
        if print {
            self.bar.println(out);
        }
    }

    pub(crate) fn inc(&self) {
        self.bar.inc(1);
    }

    pub(crate) fn set_length(&self, n: u64) {
        self.bar.set_length(n);
    }

    pub(crate) fn increase_length(&self, out: String) {
        self.bar.inc_length(1);
        self.bar.set_message(out.clone());
    }

    pub(crate) fn finish(&self, message: &'static str) {
        self.bar.finish_with_message(message);
    }
}
