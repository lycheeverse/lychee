use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct LycheeProgressBar {
    bar: ProgressBar,
}

impl LycheeProgressBar {
    const TEMPLATE: &str = "{spinner:.162} {pos}/{len:.238} {bar:.162/238} {wide_msg}";
    const DEFAULT_INCREMENT: u64 = 1;
    const TICK_INTERVAL: Duration = Duration::from_millis(500);

    pub(crate) fn new(initial_message: &'static str) -> Self {
        let bar = ProgressBar::new_spinner().with_style(
            ProgressStyle::with_template(Self::TEMPLATE)
                .expect("Valid progress bar")
                .progress_chars("━ ━"),
        );
        bar.set_length(0);
        bar.set_message(initial_message);
        bar.enable_steady_tick(Self::TICK_INTERVAL);
        LycheeProgressBar { bar }
    }

    pub(crate) fn update(&self, message: String) {
        self.inc();
        self.bar.set_message(message.clone());
    }

    pub(crate) fn inc(&self) {
        self.bar.inc(Self::DEFAULT_INCREMENT);
    }

    pub(crate) fn set_length(&self, n: u64) {
        self.bar.set_length(n);
    }

    pub(crate) fn increase_length(&self, out: String) {
        self.bar.inc_length(Self::DEFAULT_INCREMENT);
        self.bar.set_message(out.clone());
    }

    pub(crate) fn finish(&self, message: &'static str) {
        self.bar.finish_with_message(message);
    }
}
