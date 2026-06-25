use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use miette::IntoDiagnostic;

use crate::state::{ConfirmResult, GetPinResult, PinentryState, SecretBytes};
use crate::ui::PinentryUi;

// NOTE: rpassword is used for cross-platform password reading with echo
// disabled. It must be given a file path (not a reader) so it can detect
// the TTY and manipulate terminal settings on the correct fd.

/// Simple line-based TTY fallback UI.
///
/// Opens the terminal device directly, using `OPTION ttyname` from gpg-agent
/// if available, or `/dev/tty` as fallback.
pub struct TtyUi;

impl TtyUi {
    pub fn new() -> Self {
        Self
    }
}

/// Determine which terminal device to open.
fn tty_path(state: &PinentryState) -> String {
    state
        .ttyname
        .clone()
        .unwrap_or_else(|| "/dev/tty".to_string())
}

/// Open the terminal for both reading and writing.
fn open_tty(state: &PinentryState) -> miette::Result<(BufReader<File>, File)> {
    let path = tty_path(state);
    tracing::debug!("TTY: opening terminal: {path}");

    let tty_in = OpenOptions::new()
        .read(true)
        .open(&path)
        .into_diagnostic()?;
    let tty_out = OpenOptions::new()
        .write(true)
        .open(&path)
        .into_diagnostic()?;
    Ok((BufReader::new(tty_in), tty_out))
}

impl PinentryUi for TtyUi {
    fn flavor(&self) -> &str {
        "wukong:tty"
    }

    fn get_pin(&self, state: &PinentryState) -> miette::Result<GetPinResult> {
        let path = tty_path(state);
        let mut writer = OpenOptions::new()
            .write(true)
            .open(&path)
            .into_diagnostic()?;

        if let Some(ref desc) = state.description {
            writeln!(writer, "{desc}").into_diagnostic()?;
        }

        if let Some(ref err) = state.error {
            writeln!(writer, "ERROR: {err}").into_diagnostic()?;
        }

        let prompt = &state.prompt;
        write!(writer, "{prompt} ").into_diagnostic()?;
        writer.flush().into_diagnostic()?;

        // Use rpassword with file path so it gets the fd and handles
        // echo disable/restore via termios (Unix) or SetConsoleMode (Windows).
        let config = rpassword::ConfigBuilder::new()
            .input_file_path(&path)
            .output_file_path(&path)
            .build();
        let pin = rpassword::read_password_with_config(config).into_diagnostic()?;

        if pin.is_empty() {
            return Ok(GetPinResult::Closed);
        }

        Ok(GetPinResult::Pin(SecretBytes::from(pin.into_bytes())))
    }

    fn confirm(&self, state: &PinentryState) -> miette::Result<ConfirmResult> {
        let (mut reader, mut writer) = open_tty(state)?;

        if let Some(ref desc) = state.description {
            writeln!(writer, "{desc}").into_diagnostic()?;
        }

        if let Some(ref err) = state.error {
            writeln!(writer, "ERROR: {err}").into_diagnostic()?;
        }

        let ok_label = &state.ok;
        let cancel_label = &state.cancel;

        if state.notok.is_some() {
            let notok_label = state.notok.as_deref().unwrap_or("Not OK");
            write!(writer, "[{ok_label}] [{notok_label}] [{cancel_label}]? ").into_diagnostic()?;
        } else {
            write!(writer, "[{ok_label}] [{cancel_label}]? ").into_diagnostic()?;
        }
        writer.flush().into_diagnostic()?;

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => Ok(ConfirmResult::Closed),
            Ok(_) => {
                let input = line.trim().to_lowercase();
                match input.as_str() {
                    "" | "y" | "yes" | "ok" => Ok(ConfirmResult::Accepted),
                    "n" | "no" | "cancel" => Ok(ConfirmResult::Canceled),
                    _ => {
                        if state.notok.is_some() {
                            Ok(ConfirmResult::NotOk)
                        } else {
                            Ok(ConfirmResult::Canceled)
                        }
                    }
                }
            }
            Err(e) => Err(miette::miette!("failed to read input: {e}")),
        }
    }

    fn message(&self, state: &PinentryState) -> miette::Result<()> {
        let (mut reader, mut writer) = open_tty(state)?;

        if let Some(ref desc) = state.description {
            writeln!(writer, "{desc}").into_diagnostic()?;
        }

        write!(writer, "[OK] ").into_diagnostic()?;
        writer.flush().into_diagnostic()?;

        let mut line = String::new();
        reader.read_line(&mut line).into_diagnostic()?;

        Ok(())
    }
}
