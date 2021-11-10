use super::StatsWriter;
use crate::{color::color_response, stats::ResponseStats};

use anyhow::Result;
use pad::{Alignment, PadStr};
use std::fmt::{self, Display};

// Maximum padding for each entry in the final statistics output
const MAX_PADDING: usize = 20;


fn write_stat(f: &mut fmt::Formatter, title: &str, stat: usize, newline: bool) -> fmt::Result {
    let fill = title.chars().count();
    f.write_str(title)?;
    f.write_str(
        &stat
            .to_string()
            .pad(MAX_PADDING - fill, '.', Alignment::Right, false),
    )?;

    if newline {
        f.write_str("\n")?;
    }

    Ok(())
}

impl Display for ResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let separator = "-".repeat(MAX_PADDING + 1);

        writeln!(f, "\u{1f4dd} Summary")?; // 📝
        writeln!(f, "{}", separator)?;
        write_stat(f, "\u{1f50d} Total", self.total, true)?; // 🔍
        write_stat(f, "\u{2705} Successful", self.successful, true)?; // ✅
        write_stat(f, "\u{23f3} Timeouts", self.timeouts, true)?; // ⏳
        write_stat(f, "\u{1f500} Redirected", self.redirects, true)?; // 🔀
        write_stat(f, "\u{1f47b} Excluded", self.excludes, true)?; // 👻
        write_stat(f, "\u{26a0} Unknown", self.unknown, true)?; // ⚠️
        write_stat(f, "\u{1f6ab} Errors", self.errors + self.failures, false)?; // 🚫

        for (input, responses) in &self.fail_map {
            // Using leading newlines over trailing ones (e.g. `writeln!`)
            // lets us avoid extra newlines without any additional logic.
            write!(f, "\n\nErrors in {}", input)?;
            for response in responses {
                write!(f, "\n{}", color_response(response))?;
            }
        }

        Ok(())
    }
}

pub struct Detailed;

impl Detailed {
    pub(crate) fn new() -> Self {
        Detailed {}
    }
}

impl StatsWriter for Detailed {
    fn write(&self, stats: &ResponseStats) -> Result<String> {
        Ok(stats.to_string())
    }
}
