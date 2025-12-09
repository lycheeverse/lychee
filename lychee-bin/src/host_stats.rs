use anyhow::{Context, Result};
use lychee_lib::ratelimit::HostPool;

use crate::{formatters::get_host_stats_formatter, options::Config};

/// Display per-host statistics if requested
pub(crate) fn display_per_host_statistics(host_pool: &HostPool, config: &Config) -> Result<()> {
    if !config.host_stats {
        return Ok(());
    }

    let host_stats = host_pool.all_host_stats();
    let host_stats_formatter = get_host_stats_formatter(&config.format, &config.mode);

    if let Some(formatted_host_stats) = host_stats_formatter.format(host_stats)? {
        if let Some(output) = &config.output {
            // For file output, append to the existing output
            let mut file_content = std::fs::read_to_string(output).unwrap_or_default();
            file_content.push_str(&formatted_host_stats);
            std::fs::write(output, file_content)
                .context("Cannot write host stats to output file")?;
        } else {
            print!("{formatted_host_stats}");
        }
    }
    Ok(())
}
