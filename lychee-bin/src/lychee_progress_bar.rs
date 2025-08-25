use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::verbosity::Verbosity;

pub(crate) fn init_progress_bar(initial_message: &'static str) -> ProgressBar {
    let bar = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template("{spinner:.162} {pos}/{len:.238} {bar:.162/238} {wide_msg}")
            .expect("Valid progress bar")
            .progress_chars("━ ━"),
    );
    bar.set_length(0);
    bar.set_message(initial_message);
    // report status _at least_ every 500ms
    bar.enable_steady_tick(Duration::from_millis(500));
    bar
}

pub(crate) fn update_progress_bar(pb: &ProgressBar, out: String, verbose: &Verbosity) {
    pb.inc(1);
    pb.set_message(out.clone());
    if verbose.log_level() >= log::Level::Info {
        pb.println(out);
    }
}

pub(crate) fn increase_progress_bar_length(pb: &ProgressBar, out: String) {
    pb.inc_length(1);
    pb.set_message(out.clone());
}

pub(crate) fn finish_progress_bar(pb: &ProgressBar, message: &'static str) {
    pb.finish_with_message(message);
}
