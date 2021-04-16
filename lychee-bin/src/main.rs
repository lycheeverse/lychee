#![warn(clippy::all, clippy::pedantic)]
#![warn(
    absolute_paths_not_starting_with_crate,
    invalid_html_tags,
    missing_copy_implementations,
    missing_debug_implementations,
    semicolon_in_expressions_from_macros,
    unreachable_pub,
    unused_extern_crates,
    variant_size_differences,
    clippy::missing_const_for_fn
)]
#![deny(anonymous_parameters, macro_use_extern_crate, pointer_structural_match)]

#[cfg(feature = "json_output")]
use anyhow::Context;
use anyhow::{anyhow, Result};
#[cfg(feature = "indicatif")]
use indicatif::{ProgressBar, ProgressStyle};
use lychee_lib::{
    collector::{collect_links, Input},
    ClientPool,
};
use openssl_sys as _; // required for vendored-openssl feature
use ring as _; // required for apple silicon
use tokio::sync::mpsc;

#[cfg(feature = "json_output")]
mod format;
mod options;
mod stats;

#[cfg(feature = "json_output")]
use crate::format::Format;
use crate::{
    options::{Config, LycheeOptions},
    stats::ResponseStats,
};

/// A C-like enum that can be cast to `i32` and used as process exit code.
enum ExitCode {
    Success = 0,
    // TODO: exit code 1 is used for any `Result::Err` bubbled up to `main()` using the `?` operator.
    // For now, 1 acts as a catch-all for everything non-link related (including config errors),
    // until we find a way to structure the error code handling better.
    #[allow(unused)]
    UnexpectedFailure = 1,
    LinkCheckFailure = 2,
}

fn main() -> Result<()> {
    // std::process::exit doesn't guarantee that all destructors will be ran,
    // therefore we wrap "main" code in another function to guarantee that.
    // See: https://doc.rust-lang.org/stable/std/process/fn.exit.html
    // Also see: https://www.youtube.com/watch?v=zQC8T71Y8e4
    let exit_code = run_main()?;
    std::process::exit(exit_code);
}

fn run_main() -> Result<i32> {
    let opts = LycheeOptions::load_options()?;
    let (cfg, inputs) = (&opts.config, opts.inputs());

    let runtime = match cfg.threads {
        Some(threads) => {
            // We define our own runtime instead of the `tokio::main` attribute
            // since we want to make the number of threads configurable
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads)
                .enable_all()
                .build()?
        }
        None => tokio::runtime::Runtime::new()?,
    };

    runtime.block_on(run(&cfg, inputs))
}

#[cfg(feature = "indicatif")]
fn show_progress(
    progress_bar: &Option<ProgressBar>,
    response: &lychee_lib::Response,
    verbose: bool,
) {
    let out = crate::stats::color_response(&response.1);
    if let Some(pb) = progress_bar {
        pb.inc(1);
        pb.set_message(&out);
        if verbose {
            pb.println(out);
        }
    } else {
        if (response.status().is_success() || response.status().is_excluded()) && !verbose {
            return;
        }
        println!("{}", out);
    }
}

#[cfg(feature = "json_output")]
fn fmt(stats: &ResponseStats, format: Format) -> Result<String> {
    match format {
        Format::String => Ok(stats.to_string()),
        Format::Json => serde_json::to_string_pretty(&stats).map_err(|e| e.into()),
    }
}

fn print_stats(cfg: &Config, stats: &ResponseStats) -> Result<()> {
    #[cfg(feature = "json_output")]
    let stats_formatted = fmt(&stats, cfg.format)?;
    #[cfg(not(feature = "json_output"))]
    let stats_formatted = stats.to_string();

    #[cfg(feature = "json_output")]
    if let Some(output) = &cfg.output {
        return std::fs::write(output, stats_formatted)
            .context("Cannot write status output to file");
    }
    if cfg.verbose && !stats.is_empty() {
        // separate summary from the verbose list of links above
        println!();
    }
    // we assume that the formatted stats don't have a final newline
    println!("{}", stats_formatted);

    Ok(())
}

async fn run(cfg: &Config, inputs: Vec<Input>) -> Result<i32> {
    let client = cfg.build()?;

    let max_concurrency = cfg.max_concurrency;

    let links = collect_links(
        &inputs,
        cfg.base_url.clone(),
        cfg.skip_missing,
        max_concurrency,
    )
    .await
    .map_err(|e| anyhow!(e))?;

    #[cfg(feature = "indicatif")]
    let pb = if cfg.no_progress {
        None
    } else {
        let bar =
            ProgressBar::new(links.len() as u64).with_style(ProgressStyle::default_bar().template(
                "{spinner:.red.bright} {pos}/{len:.dim} [{elapsed_precise}] {bar:25} {wide_msg}",
            ));
        bar.enable_steady_tick(100);
        Some(bar)
    };

    let (send_req, recv_req) = mpsc::channel(max_concurrency);
    let (send_resp, mut recv_resp) = mpsc::channel(max_concurrency);

    let mut stats = ResponseStats::new();

    #[cfg(feature = "indicatif")]
    let bar = pb.clone();
    tokio::spawn(async move {
        for link in links {
            #[cfg(feature = "indicatif")]
            if let Some(pb) = &bar {
                pb.set_message(&link.to_string());
            };
            send_req.send(link).await.unwrap();
        }
    });

    // Start receiving requests
    tokio::spawn(async move {
        let clients = vec![client; max_concurrency];
        let mut clients = ClientPool::new(send_resp, recv_req, clients);
        clients.listen().await;
    });

    while let Some(response) = recv_resp.recv().await {
        #[cfg(feature = "indicatif")]
        show_progress(&pb, &response, cfg.verbose);
        stats.add(response);
    }

    // Note that print statements may interfere with the progress bar, so this
    // must go before printing the stats
    #[cfg(feature = "indicatif")]
    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    print_stats(cfg, &stats)?;

    if stats.is_success() {
        Ok(ExitCode::Success as i32)
    } else {
        Ok(ExitCode::LinkCheckFailure as i32)
    }
}
