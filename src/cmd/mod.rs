use std::fs::OpenOptions;
use std::path::PathBuf;

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use miette::IntoDiagnostic;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod completions;
mod serve;

#[derive(Debug, Parser)]
#[command(
    bin_name = env!("CARGO_PKG_NAME"),
    version,
    about = env!("CARGO_PKG_DESCRIPTION"),
)]
pub struct App {
    #[command(subcommand)]
    command: Option<Command>,

    /// The verbosity level (default: trace)
    #[command(flatten)]
    verbose: Verbosity,

    /// Write logs to a file instead of the default <exe>.log
    #[arg(long, global = true, value_name = "PATH")]
    log_file: Option<PathBuf>,

    /// Serve flags
    #[command(flatten)]
    serve: serve::Args,
}

#[derive(Debug, Parser)]
pub enum Command {
    /// Run the pinentry server
    Serve(serve::Args),

    /// Generate shell completions
    Completions(completions::Args),
}

/// Resolve the log file path: explicit `--log-file` wins, otherwise `<exe>.log`.
fn resolve_log_file(explicit: Option<&PathBuf>) -> miette::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.clone());
    }
    let exe = std::env::current_exe().into_diagnostic()?;
    Ok(exe.with_extension("log"))
}

/// Setup the global logger with the given level filter.
///
/// Logs always go to a file (default: `<exe>.log`). This is essential for
/// debugging since the TUI redirects stderr.
fn setup_logger(level_filter: LevelFilter, log_file: &PathBuf) -> miette::Result<()> {
    let low_level_filter = match level_filter {
        LevelFilter::OFF => LevelFilter::OFF,
        LevelFilter::ERROR => LevelFilter::ERROR,
        LevelFilter::WARN => LevelFilter::WARN,
        LevelFilter::INFO => LevelFilter::WARN,
        LevelFilter::DEBUG => LevelFilter::INFO,
        LevelFilter::TRACE => LevelFilter::TRACE,
    };

    let mut layer_env_filter = EnvFilter::builder()
        .with_default_directive(level_filter.into())
        .from_env()
        .into_diagnostic()?;

    layer_env_filter = layer_env_filter
        .add_directive(
            format!("hyper_util={}", low_level_filter)
                .parse()
                .into_diagnostic()?,
        )
        .add_directive(
            format!("reqwest={}", low_level_filter)
                .parse()
                .into_diagnostic()?,
        );

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .into_diagnostic()?;

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file)
        .with_ansi(false)
        .without_time();

    tracing_subscriber::registry()
        .with(layer_env_filter)
        .with(file_layer)
        .init();

    Ok(())
}

/// CLI entry point
pub async fn start() -> miette::Result<()> {
    let args = App::parse();

    // Default to TRACE unless the user explicitly set a verbosity level.
    let level = if args.verbose.is_present() {
        args.verbose.tracing_level_filter()
    } else {
        LevelFilter::TRACE
    };

    let log_file = resolve_log_file(args.log_file.as_ref())?;
    setup_logger(level, &log_file)?;
    tracing::info!("logging to {}", log_file.display());

    match args.command {
        Some(Command::Serve(args)) => serve::execute(args).await?,
        Some(Command::Completions(args)) => completions::execute(args).await?,
        None => serve::execute(args.serve).await?,
    }
    Ok(())
}
