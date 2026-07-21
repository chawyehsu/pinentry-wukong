use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use miette::IntoDiagnostic;

use crate::state::{ConfirmResult, GetPinResult, PinentryState, SecretBytes};

fn tty_path(state: &PinentryState) -> String {
    state
        .ttyname
        .clone()
        .unwrap_or_else(|| "/dev/tty".to_string())
}

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

pub(super) fn get_pin(state: &PinentryState) -> miette::Result<GetPinResult> {
    let mut writer = {
        let path = tty_path(state);
        OpenOptions::new()
            .write(true)
            .open(&path)
            .into_diagnostic()?
    };

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").into_diagnostic()?;
    }
    if let Some(ref err) = state.error {
        writeln!(writer, "ERROR: {err}").into_diagnostic()?;
    }
    let prompt = &state.prompt;
    write!(writer, "{prompt} ").into_diagnostic()?;
    writer.flush().into_diagnostic()?;

    let pin = {
        let path = tty_path(state);
        let config = rpassword::ConfigBuilder::new()
            .input_file_path(&path)
            .output_file_path(&path)
            .build();
        rpassword::read_password_with_config(config).into_diagnostic()?
    };

    if pin.is_empty() {
        return Ok(GetPinResult::Closed);
    }
    Ok(GetPinResult::Pin(SecretBytes::from(pin.into_bytes())))
}

pub(super) fn confirm(state: &PinentryState) -> miette::Result<ConfirmResult> {
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
    reader.read_line(&mut line).into_diagnostic()?;

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

pub(super) fn message(state: &PinentryState) -> miette::Result<()> {
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
