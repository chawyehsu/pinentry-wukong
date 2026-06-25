use clap::Parser;

use crate::keychain::Keychain;
use crate::keychain::keyring::KeyringKeychain;
use crate::server;
use crate::ui::detect::{self, UiMode};

/// Run the pinentry server
///
/// Reads Assuan commands from reader and writes responses to writer.
/// Typically invoked by gpg-agent as a subprocess.
#[derive(Parser, Debug, Default)]
pub struct Args {
    /// Turn on debugging output
    #[arg(short, long)]
    pub debug: bool,

    /// X display name (ignored on non-X11)
    #[arg(short = 'D', long, value_name = "DISPLAY")]
    pub display: Option<String>,

    /// TTY terminal node name
    #[arg(short = 'T', long, value_name = "FILE")]
    pub ttyname: Option<String>,

    /// TTY terminal type
    #[arg(short = 'N', long, value_name = "NAME")]
    pub ttytype: Option<String>,

    /// TTY LC_CTYPE value
    #[arg(short = 'C', long, value_name = "STRING")]
    pub lc_ctype: Option<String>,

    /// TTY LC_MESSAGES value
    #[arg(short = 'M', long, value_name = "STRING")]
    pub lc_messages: Option<String>,

    /// Input timeout in seconds (default: 60)
    #[arg(short = 'o', long, value_name = "SECS")]
    pub timeout: Option<u32>,

    /// Grab keyboard only when the window is focused
    #[arg(short = 'g', long)]
    pub no_global_grab: bool,

    /// Force a specific UI mode
    #[arg(long, value_name = "MODE")]
    pub ui: Option<UiMode>,

    /// Disable keychain integration
    #[arg(long)]
    pub no_keychain: bool,
}

pub async fn execute(args: Args) -> miette::Result<()> {
    let ui_mode = args.ui.unwrap_or_else(detect::detect_ui_mode);
    tracing::info!("using UI mode: {ui_mode}");

    // Set up keychain
    let keychain: Option<Box<dyn Keychain>> = if args.no_keychain {
        tracing::info!("keychain disabled");
        None
    } else {
        tracing::info!("keychain enabled (OS credential store)");
        Some(Box::new(KeyringKeychain::new()))
    };

    let grab = !args.no_global_grab;
    let timeout = args.timeout.unwrap_or(60);
    let ui = ui_mode.create_ui();

    server::start(&*ui, grab, timeout, keychain)?;

    Ok(())
}
