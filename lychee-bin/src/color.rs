use console::Style;
use once_cell::sync::Lazy;

pub(crate) static NORMAL: Lazy<Style> = Lazy::new(Style::new);
pub(crate) static DIM: Lazy<Style> = Lazy::new(|| Style::new().dim());

pub(crate) static GREEN: Lazy<Style> = Lazy::new(|| Style::new().green().bright());
pub(crate) static BOLD_GREEN: Lazy<Style> = Lazy::new(|| Style::new().green().bold().bright());
pub(crate) static YELLOW: Lazy<Style> = Lazy::new(|| Style::new().yellow().bright());
pub(crate) static BOLD_YELLOW: Lazy<Style> = Lazy::new(|| Style::new().yellow().bold().bright());
pub(crate) static PINK: Lazy<Style> = Lazy::new(|| Style::new().color256(197).bright());
pub(crate) static BOLD_PINK: Lazy<Style> = Lazy::new(|| Style::new().color256(197).bold().bright());

// Write output using predefined colors
macro_rules! color {
    ($f:ident, $color:ident, $text:tt, $($tts:tt)*) => {
        write!($f, "{}", $color.apply_to(format!($text, $($tts)*)))
    };
}

pub(crate) use color;
