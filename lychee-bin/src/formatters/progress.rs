use indicatif::{ProgressBar as Bar, ProgressStyle};
use std::time::Duration;

#[derive(Clone)]
struct ProgressConfig {
    pub template: &'static str,
    pub tick_interval: Duration,
    pub progress_chars: &'static str,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            template: "{spinner:.162} {pos}/{len:.238} {bar:.162/238} {wide_msg}",
            tick_interval: Duration::from_millis(500),
            progress_chars: "━ ━",
        }
    }
}

#[derive(Clone)]
/// Report progress to the CLI during link collection and checking.
pub(crate) struct ProgressBar {
    bar: Bar,
}

impl ProgressBar {
    pub(crate) fn new(initial_message: &'static str) -> Self {
        let config = ProgressConfig::default();

        let style = ProgressStyle::with_template(config.template)
            .expect("Valid progress bar")
            .progress_chars(config.progress_chars);

        let bar = Bar::new_spinner().with_style(style);

        bar.set_length(0);
        bar.set_message(initial_message);
        bar.enable_steady_tick(config.tick_interval);

        ProgressBar { bar }
    }

    pub(crate) fn update(&self, message: Option<String>) {
        self.bar.inc(1);
        if let Some(msg) = message {
            self.bar.set_message(msg);
        }
    }

    pub(crate) fn set_length(&self, n: u64) {
        self.bar.set_length(n);
    }

    pub(crate) fn inc_length(&self, n: u64) {
        self.bar.inc_length(n);
    }

    pub(crate) fn finish(&self, message: &'static str) {
        self.bar.finish_with_message(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_initializes_correctly() {
        let pb = ProgressBar::new("Start");
        assert_eq!(pb.bar.length(), Some(0));
        assert_eq!(pb.bar.message(), "Start");
    }

    #[test]
    fn test_update_increments_and_changes_message() {
        let pb = ProgressBar::new("First message");

        pb.update(None); // update without message
        assert_eq!(pb.bar.position(), 1);
        assert_eq!(pb.bar.message(), "First message");

        pb.update(Some("Second message".to_string()));
        assert_eq!(pb.bar.position(), 2);
        assert_eq!(pb.bar.message(), "Second message");
    }

    #[test]
    fn test_finish_closes_bar_and_sets_final_message() {
        let pb = ProgressBar::new("Running");
        pb.set_length(5);
        pb.update(None);
        pb.finish("Done");

        assert!(pb.bar.is_finished());
        assert_eq!(pb.bar.message(), "Done");
    }
}
