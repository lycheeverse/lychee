//! Defines the colors used in the output of the CLI.

use std::sync::LazyLock;

use console::Style;
use log::Level;

pub(crate) static NORMAL: LazyLock<Style> = LazyLock::new(Style::new);
pub(crate) static DIM: LazyLock<Style> = LazyLock::new(|| Style::new().dim());

pub(crate) static GREEN: LazyLock<Style> =
    LazyLock::new(|| Style::new().color256(2).bold().bright());
pub(crate) static BOLD_GREEN: LazyLock<Style> =
    LazyLock::new(|| Style::new().color256(82).bold().bright());
pub(crate) static YELLOW: LazyLock<Style> = LazyLock::new(|| Style::new().yellow().bright());
pub(crate) static BOLD_YELLOW: LazyLock<Style> =
    LazyLock::new(|| Style::new().yellow().bold().bright());
pub(crate) static PINK: LazyLock<Style> = LazyLock::new(|| Style::new().color256(197));
pub(crate) static BOLD_PINK: LazyLock<Style> = LazyLock::new(|| Style::new().color256(197).bold());

// Used for debug log messages
pub(crate) static BLUE: LazyLock<Style> = LazyLock::new(|| Style::new().blue().bright());

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
