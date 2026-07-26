use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use assuan::ErrorCode;
use miette::IntoDiagnostic;

use crate::state::{PinentryState, SecretBytes};

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

pub(super) fn get_pin(state: &PinentryState) -> Result<SecretBytes, ErrorCode> {
    let mut writer = {
        let path = tty_path(state);
        OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|_| ErrorCode::GENERAL)?
    };

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    if let Some(ref err) = state.error {
        writeln!(writer, "ERROR: {err}").map_err(|_| ErrorCode::GENERAL)?;
    }
    let prompt = &state.prompt;
    write!(writer, "{prompt} ").map_err(|_| ErrorCode::GENERAL)?;
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    let pin = {
        let path = tty_path(state);
        let config = rpassword::ConfigBuilder::new()
            .input_file_path(&path)
            .output_file_path(&path)
            .build();
        rpassword::read_password_with_config(config).map_err(|_| ErrorCode::GENERAL)?
    };

    if pin.is_empty() {
        return Err(ErrorCode::CANCELED);
    }
    Ok(SecretBytes::from(pin.into_bytes()))
}

pub(super) fn confirm(state: &PinentryState) -> Result<(), ErrorCode> {
    let (mut reader, mut writer) = open_tty(state).map_err(|_| ErrorCode::GENERAL)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    if let Some(ref err) = state.error {
        writeln!(writer, "ERROR: {err}").map_err(|_| ErrorCode::GENERAL)?;
    }

    let ok_label = &state.ok;
    let cancel_label = &state.cancel;
    if state.notok.is_some() {
        let notok_label = state.notok.as_deref().unwrap_or("Not OK");
        write!(writer, "[{ok_label}] [{notok_label}] [{cancel_label}]? ")
            .map_err(|_| ErrorCode::GENERAL)?;
    } else {
        write!(writer, "[{ok_label}] [{cancel_label}]? ").map_err(|_| ErrorCode::GENERAL)?;
    }
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|_| ErrorCode::GENERAL)?;

    let input = line.trim().to_lowercase();
    match input.as_str() {
        "" | "y" | "yes" | "ok" => Ok(()),
        "n" | "no" | "cancel" => Err(ErrorCode::CANCELED),
        _ => {
            if state.notok.is_some() {
                Err(ErrorCode::NOT_CONFIRMED)
            } else {
                Err(ErrorCode::CANCELED)
            }
        }
    }
}

pub(super) fn message(state: &PinentryState) -> Result<(), ErrorCode> {
    let (mut reader, mut writer) = open_tty(state).map_err(|_| ErrorCode::GENERAL)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    write!(writer, "[OK] ").map_err(|_| ErrorCode::GENERAL)?;
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|_| ErrorCode::GENERAL)?;
    Ok(())
}
