//! Defines the colors used in the output of the CLI.

use console::Style;
use log::Level;
use once_cell::sync::Lazy;

pub(crate) static NORMAL: Lazy<Style> = Lazy::new(Style::new);
pub(crate) static DIM: Lazy<Style> = Lazy::new(|| Style::new().dim());

pub(crate) static GREEN: Lazy<Style> = Lazy::new(|| Style::new().color256(2).bold().bright());
pub(crate) static BOLD_GREEN: Lazy<Style> = Lazy::new(|| Style::new().color256(82).bold().bright());
pub(crate) static YELLOW: Lazy<Style> = Lazy::new(|| Style::new().yellow().bright());
pub(crate) static BOLD_YELLOW: Lazy<Style> = Lazy::new(|| Style::new().yellow().bold().bright());
pub(crate) static PINK: Lazy<Style> = Lazy::new(|| Style::new().color256(197));
pub(crate) static BOLD_PINK: Lazy<Style> = Lazy::new(|| Style::new().color256(197).bold());

// Used for debug log messages
pub(crate) static BLUE: Lazy<Style> = Lazy::new(|| Style::new().blue().bright());

// Write output using predefined colors
macro_rules! color {
    ($f:ident, $color:ident, $text:tt, $($tts:tt)*) => {
        write!($f, "{}", $color.apply_to(format!($text, $($tts)*)))
    };
}

/// Returns the appropriate color for a given log level.
pub(crate) fn color_for_level(level: Level) -> &'static Style {
    match level {
        Level::Error => &BOLD_PINK,
        Level::Warn => &BOLD_YELLOW,
        Level::Info | Level::Debug => &BLUE,
        Level::Trace => &DIM,
    }
}

pub(crate) use color;
