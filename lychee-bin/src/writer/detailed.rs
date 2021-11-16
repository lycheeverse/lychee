use super::StatsWriter;
use crate::stats::{color_response, ResponseStats};

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

impl Display for DetailedResponseStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.0;
        let separator = "-".repeat(MAX_PADDING + 1);

        writeln!(f, "\u{1f4dd} Summary")?; // 📝
        writeln!(f, "{}", separator)?;
        write_stat(f, "\u{1f50d} Total", stats.total, true)?; // 🔍
        write_stat(f, "\u{2705} Successful", stats.successful, true)?; // ✅
        write_stat(f, "\u{23f3} Timeouts", stats.timeouts, true)?; // ⏳
        write_stat(f, "\u{1f500} Redirected", stats.redirects, true)?; // 🔀
        write_stat(f, "\u{1f47b} Excluded", stats.excludes, true)?; // 👻
        write_stat(f, "\u{2753} Unknown", stats.unknown, true)?; //❓
        write_stat(f, "\u{1f6ab} Errors", stats.errors + stats.failures, false)?; // 🚫

        for (input, responses) in &stats.fail_map {
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

/// Wrap as newtype because multiple `Display` implementations are not allowed
/// for `ResponseStats`
struct DetailedResponseStats(ResponseStats);

pub(crate) struct Detailed;

impl Detailed {
    pub(crate) const fn new() -> Self {
        Detailed {}
    }
}

impl StatsWriter for Detailed {
    fn write(&self, stats: ResponseStats) -> Result<String> {
        let detailed = DetailedResponseStats(stats);
        Ok(detailed.to_string())
    }
}
