use std::io::Write;

use assuan::ErrorCode;

use crate::state::{PinentryState, SecretBytes};
use crate::ui::windows::{resolve_console_handles, write_error};

pub(super) fn get_pin(state: &PinentryState) -> Result<SecretBytes, ErrorCode> {
    let (mut writer, reader, _source) =
        resolve_console_handles(state).map_err(|_| ErrorCode::GENERAL)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    if let Some(ref err) = state.error {
        write_error(&mut writer, err)?;
    }
    let prompt = &state.prompt;
    write!(writer, "{prompt} ").map_err(|_| ErrorCode::GENERAL)?;
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    let pin = SecretBytes::from(reader.read_line_bytes(state.timeout)?);

    // Echo was disabled, so the user's Enter didn't produce a visible newline
    writeln!(writer).map_err(|_| ErrorCode::GENERAL)?;

    if pin.is_empty() {
        return Err(ErrorCode::CANCELED);
    }
    Ok(pin)
}

pub(super) fn confirm(state: &PinentryState) -> Result<(), ErrorCode> {
    let (mut writer, reader, _source) =
        resolve_console_handles(state).map_err(|_| ErrorCode::GENERAL)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    if let Some(ref err) = state.error {
        write_error(&mut writer, err)?;
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

    let line = reader.read_line(state.timeout)?;

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
    let (mut writer, reader, _source) =
        resolve_console_handles(state).map_err(|_| ErrorCode::GENERAL)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    if let Some(ref err) = state.error {
        write_error(&mut writer, err)?;
    }
    write!(writer, "[OK] ").map_err(|_| ErrorCode::GENERAL)?;
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    reader.read_line(state.timeout)?;
    Ok(())
}
