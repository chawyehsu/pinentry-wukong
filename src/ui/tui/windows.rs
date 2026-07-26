use std::io::Write;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, INPUT_RECORD, KEY_EVENT,
    KEY_EVENT_RECORD, ReadConsoleInputW, SetConsoleMode,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use super::Key;
use crate::state::PinentryState;
use crate::ui::windows::{
    ConsoleHandle, ConsoleModeGuard, ConsoleSource, open_console_handle, redirect_std_to_console,
    resolve_console_source, restore_std_handles,
};

pub(super) struct TtyGuard {
    saved_stdin: HANDLE,
    saved_stdout: HANDLE,
    conin: HANDLE,
    _mode_guard: ConsoleModeGuard,
    _writer: ConsoleHandle,
    _reader: ConsoleHandle,
    _source: ConsoleSource,
}

impl TtyGuard {
    pub(super) fn redirect(state: &PinentryState) -> miette::Result<Self> {
        let source = resolve_console_source(state)?;
        let writer = open_console_handle(&source, "CONOUT$", 0x40000000)?;
        let reader = open_console_handle(&source, "CONIN$", 0xC0000000)?;

        let _ = std::io::stdout().flush();

        let (saved_stdin, saved_stdout) = redirect_std_to_console(&reader, &writer)?;

        // Disable echo, line input, and processed input on CONIN$.
        // Clearing ENABLE_PROCESSED_INPUT ensures Ctrl+C arrives as a
        // KEY_EVENT for poll_key rather than terminating the process.
        // cleanup_terminal restores all three flags on exit.
        let mode_guard = reader
            .set_mode(|m| m & !ENABLE_ECHO_INPUT & !ENABLE_LINE_INPUT & !ENABLE_PROCESSED_INPUT)
            .map_err(|_| miette::miette!("failed to set console mode"))?;

        let conin = reader.raw();
        tracing::debug!("TUI: TtyGuard created, console handles ready (conin={conin:?})");
        Ok(Self {
            saved_stdin,
            saved_stdout,
            conin,
            _mode_guard: mode_guard,
            _writer: writer,
            _reader: reader,
            _source: source,
        })
    }

    pub(super) fn handle(&self) -> HANDLE {
        self.conin
    }
}

impl Drop for TtyGuard {
    fn drop(&mut self) {
        restore_std_handles(self.saved_stdin, self.saved_stdout);
        // _mode_guard restores original CONIN$ mode.
        // _reader, _writer close their handles.
        // _source detaches the console (FreeConsole) if Ttyname/Allocated.
    }
}

pub(super) fn cleanup_terminal(conin: HANDLE) -> miette::Result<()> {
    // Restore the console mode on the CONIN$ handle. We do this here (not in
    // TtyGuard::drop) because the guard must stay alive until after this call
    // — the CONIN$ handle is only valid while the console is attached.
    //
    // SAFETY: conin is the CONIN$ handle opened in TtyGuard::redirect.
    // The handle is valid because the guard (and its _source) are still alive.
    unsafe {
        SetConsoleMode(
            conin,
            ENABLE_PROCESSED_INPUT | ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT,
        );
    }
    Ok(())
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
