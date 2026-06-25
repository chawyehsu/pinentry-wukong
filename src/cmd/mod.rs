use std::env;
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

    /// The verbosity level
    #[command(flatten)]
    verbose: Verbosity,

    /// Write logs to a file instead of stderr (useful for debugging TUI mode)
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

/// Setup the global logger with the given level filter.
///
/// If `log_file` is specified, logs go to that file instead of stderr.
/// This is essential for debugging since the TUI redirects stderr.
fn setup_logger(level_filter: LevelFilter, log_file: Option<&PathBuf>) -> miette::Result<()> {
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

    if let Some(path) = log_file {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .into_diagnostic()?;

        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .without_time();

        tracing_subscriber::registry()
            .with(layer_env_filter)
            .with(file_layer)
            .init();
    } else {
        let layer_fmt = tracing_subscriber::fmt::layer().without_time();

        tracing_subscriber::registry()
            .with(layer_env_filter)
            .with(layer_fmt)
            .init();
    }

    Ok(())
}

/// CLI entry point
pub async fn start() -> miette::Result<()> {
    let args = App::parse();
    setup_logger(args.verbose.tracing_level_filter(), args.log_file.as_ref())?;

    match args.command {
        Some(Command::Serve(args)) => serve::execute(args).await?,
        Some(Command::Completions(args)) => completions::execute(args).await?,
        None => serve::execute(args.serve).await?,
    }
    Ok(())
}
