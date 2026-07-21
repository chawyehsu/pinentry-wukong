use std::fs::OpenOptions;
use std::path::PathBuf;

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use miette::IntoDiagnostic;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod completions;
mod config;
mod serve;

use crate::config::{Config, resolve_config_path, resolve_log_path};

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

    /// Enable debug logging (equivalent to -vvv)
    #[arg(long, global = true)]
    debug: bool,

    /// Path to a custom config file
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Enable system keyring for secret management (default)
    #[arg(long, global = true)]
    keyring: bool,

    /// Disable system keyring for secret management
    #[arg(long, global = true, conflicts_with = "keyring")]
    no_keyring: bool,

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

    /// Manage configuration
    Config(config::Args),
}

/// Setup the global logger with the given level filter.
///
/// Logs always go to a file at the resolved log path.
/// This is essential for debugging since the TUI redirects stderr.
fn setup_logger(level_filter: LevelFilter) -> miette::Result<()> {
    let log_path = resolve_log_path();

    // Ensure parent directory exists
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }

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
        .open(&log_path)
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

/// Parse the log level string from config into a `LevelFilter`.
fn parse_level_filter(level: &str) -> LevelFilter {
    match level {
        "OFF" => LevelFilter::OFF,
        "ERROR" => LevelFilter::ERROR,
        "WARN" => LevelFilter::WARN,
        "INFO" => LevelFilter::INFO,
        "DEBUG" => LevelFilter::DEBUG,
        "TRACE" => LevelFilter::TRACE,
        _ => LevelFilter::INFO, // fallback (should not happen after validation)
    }
}

/// CLI entry point
pub async fn start() -> miette::Result<()> {
    let args = App::parse();

    // Load config file
    let config_path = resolve_config_path(args.config.as_deref());
    let cfg = if config_path.exists() {
        Config::load(&config_path)?
    } else {
        Config::default()
    };

    // Handle --debug vs -v/-q conflict
    // is_present() returns true only when the user explicitly passed -v/-q;
    // log_level() alone returns Some(Error) by default in clap-verbosity-flag.
    if args.debug
        && args.verbose.is_present()
        && let Some(cli_level) = args.verbose.log_level()
        && cli_level != log::Level::Debug
    {
        return Err(miette::miette!(
            "cannot use --debug with -v/-q (conflicting verbosity levels)"
        ));
    }

    // Resolve effective log level and whether logging is enabled
    let (level_filter, logging_enabled) = if args.debug {
        // --debug flag explicitly enables debug logging
        (LevelFilter::DEBUG, true)
    } else if args.verbose.is_present() {
        let cli_level = args.verbose.tracing_level_filter();
        // any `-q` flag will set the level to OFF, which disables logging
        let enabled = cli_level != LevelFilter::OFF;
        (cli_level, enabled)
    } else {
        // Use config values
        let level = parse_level_filter(&cfg.logging.level);
        (level, cfg.logging.enabled)
    };

    // Only set up logger if logging is enabled
    if logging_enabled && level_filter != LevelFilter::OFF {
        setup_logger(level_filter)?;
    }

    // Resolve effective keyring setting
    let keyring = if args.no_keyring {
        false
    } else if args.keyring {
        true
    } else {
        cfg.general.keyring
    };

    match args.command {
        Some(Command::Serve(serve_args)) => {
            serve::execute(serve_args, &cfg, keyring).await?;
        }
        Some(Command::Completions(completions_args)) => {
            completions::execute(completions_args).await?;
        }
        Some(Command::Config(config_args)) => {
            config::execute(config_args, args.config.as_deref()).await?;
        }
        None => {
            serve::execute(args.serve, &cfg, keyring).await?;
        }
    }
    Ok(())
}
