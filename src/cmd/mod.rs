use std::env;

use clap::Parser;
use clap_verbosity_flag::Verbosity;

use crate::observability::setup_logger;

mod completions;

#[derive(Debug, Parser)]
#[command(
    bin_name = env!("CARGO_PKG_NAME"),
    version,
    about = env!("CARGO_PKG_DESCRIPTION"),
)]
#[clap(arg_required_else_help = true)]
pub struct App {
    #[command(subcommand)]
    command: Command,

    /// The verbosity level
    #[command(flatten)]
    verbose: Verbosity,
}

#[derive(Debug, Parser)]
pub enum Command {
    Completions(completions::Args),
}

/// CLI entry point
pub async fn start() -> miette::Result<()> {
    let args = App::parse();
    setup_logger(args.verbose.tracing_level_filter())?;

    match args.command {
        Command::Completions(args) => completions::execute(args).await?,
    }
    Ok(())
}
