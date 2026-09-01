use std::io::Write;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Console::{
    INPUT_RECORD, KEY_EVENT, KEY_EVENT_RECORD, ReadConsoleInputW,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use super::Key;
use crate::state::PinentryState;
use crate::ui::windows::{
    ConsoleHandle, ConsoleSource, open_console_handle, redirect_std_to_console,
    resolve_console_source, restore_std_handles,
};

pub(super) struct TtyGuard {
    saved_stdin: HANDLE,
    saved_stdout: HANDLE,
    conin: HANDLE,
    writer: Option<ConsoleHandle>,
    _reader: ConsoleHandle,
    _source: ConsoleSource,
}

impl TtyGuard {
    pub(super) fn redirect(state: &PinentryState) -> miette::Result<Self> {
        let source = resolve_console_source(state)?;
        let writer = open_console_handle(&source, "CONOUT$", GENERIC_READ | GENERIC_WRITE)?;
        let reader = open_console_handle(&source, "CONIN$", GENERIC_READ | GENERIC_WRITE)?;

        let _ = std::io::stdout().flush();

        let (saved_stdin, saved_stdout) = redirect_std_to_console(&reader, &writer)?;

        let conin = reader.raw();
        tracing::debug!("TUI: TtyGuard created, console handles ready (conin={conin:?})");
        Ok(Self {
            saved_stdin,
            saved_stdout,
            conin,
            writer: Some(writer),
            _reader: reader,
            _source: source,
        })
    }

    pub(super) fn handle(&self) -> HANDLE {
        self.conin
    }

    pub(super) fn take_writer(&mut self) -> miette::Result<ConsoleHandle> {
        self.writer
            .take()
            .ok_or_else(|| miette::miette!("TUI console writer was already taken"))
    }
}

impl Drop for TtyGuard {
    fn drop(&mut self) {
        restore_std_handles(self.saved_stdin, self.saved_stdout);
    }
}

fn input_record_to_key(record: &INPUT_RECORD) -> Option<Key> {
    if record.EventType != KEY_EVENT as u16 {
        return None;
    }
    let key: KEY_EVENT_RECORD = unsafe { record.Event.KeyEvent };
    if key.bKeyDown == 0 {
        return None;
    }
    let c = unsafe { key.uChar.UnicodeChar };
    // Ctrl+C (ETX, U+0003)
    if c == 0x0003 {
        return Some(Key::CtrlC);
    }
    match c {
        0x000D | 0x000A => Some(Key::Enter),
        0x0008 | 0x007F => Some(Key::Backspace),
        0x0009 => {
            let shift = key.dwControlKeyState & 0x0010 != 0; // SHIFT_PRESSED
            if shift {
                Some(Key::BackTab)
            } else {
                Some(Key::Tab)
            }
        }
        0x001B => Some(Key::Esc),
        0x0000 => None, // control character we don't handle
        c if (0xD800..=0xDBFF).contains(&c) => None, // high surrogate, wait for low
        c => char::from_u32(c as u32).map(Key::Char),
    }
}

pub(super) fn poll_key(conin: HANDLE, timeout: Duration) -> Option<Key> {
    let ms = timeout.as_millis() as u32;
    match unsafe { WaitForSingleObject(conin, ms) } {
        WAIT_OBJECT_0 => {
            let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut events_read: u32 = 0;
            if unsafe { ReadConsoleInputW(conin, &mut record, 1, &mut events_read) } == 0 {
                return None;
            }
            if events_read == 0 {
                return None;
            }
            input_record_to_key(&record)
        }
        WAIT_TIMEOUT => None,
        _ => None,
    }
}
