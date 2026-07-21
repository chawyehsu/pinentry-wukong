use clap::Parser;

use crate::config::Config;
use crate::keychain::Keychain;
use crate::keychain::keyring::KeyringKeychain;
use crate::server;
use crate::ui::detect::UiMode;

/// Run the pinentry server
///
/// Reads Assuan commands from reader and writes responses to writer.
/// Typically invoked by gpg-agent as a subprocess.
#[derive(Parser, Debug, Default)]
pub struct Args {
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
}

pub async fn execute(args: Args, cfg: &Config, keyring: bool) -> miette::Result<()> {
    // Resolve UI mode: CLI arg > config > auto-detect
    let ui_mode = match args.ui {
        Some(mode) => mode,
        None => match cfg.general.ui.as_deref() {
            Some(mode_str) => mode_str.parse::<UiMode>().unwrap_or(UiMode::Auto),
            None => UiMode::Auto,
        },
    };
    let ui_mode = ui_mode.resolve();
    tracing::info!("using UI mode: {ui_mode}");

    // Set up keychain
    let keychain: Option<Box<dyn Keychain>> = if !keyring {
        tracing::info!("keyring disabled");
        None
    } else {
        tracing::info!("keyring enabled (OS credential store)");
        Some(Box::new(KeyringKeychain::new()))
    };

    let grab = !args.no_global_grab;

    // Resolve timeout: CLI arg > config > hardcoded default (60)
    let timeout = args.timeout.or(cfg.general.timeout).unwrap_or(60);

    let ui = ui_mode.create_ui();

    server::start(&*ui, grab, timeout, keychain)?;

    Ok(())
}
