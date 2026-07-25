use std::io::Write;
use std::time::Instant;

use assuan::ErrorCode;
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AllocConsole, AttachConsole, CONSOLE_MODE, ENABLE_PROCESSED_INPUT, FreeConsole, GetConsoleMode,
    GetStdHandle, INPUT_RECORD, KEY_EVENT, KEY_EVENT_RECORD, ReadConsoleInputW, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE, SetConsoleMode, WriteConsoleW,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use crate::state::{PinentryState, SecretBytes};

/// How the Windows console was obtained.
enum ConsoleSource {
    /// stdin is already a real console (`GetConsoleMode` succeeds).
    Direct,
    /// `OPTION ttyname` provided `/conhost/<pid>` — attached via `AttachConsole`.
    Ttyname { pid: u32 },
    /// `AllocConsole()` created a new console window (last resort).
    Allocated,
}

impl Drop for ConsoleSource {
    fn drop(&mut self) {
        match self {
            ConsoleSource::Ttyname { pid } => {
                tracing::debug!("ConsoleSource: releasing ttyname console from PID {pid}");
                unsafe {
                    FreeConsole();
                }
            }
            ConsoleSource::Allocated => {
                tracing::debug!("ConsoleSource: releasing allocated console");
                unsafe {
                    FreeConsole();
                }
            }
            ConsoleSource::Direct => {}
        }
    }
}

/// RAII wrapper for Windows console handles.
///
/// Tracks whether the handle is borrowed (std handle — must NOT be closed)
/// or owned (from CreateFileW — must be closed on drop).
enum ConsoleHandle {
    /// Borrowed from GetStdHandle — do not close.
    Borrowed(HANDLE),
    /// Owned — created via CreateFileW, must be closed.
    Owned(HANDLE),
}

/// RAII guard that restores the original console mode on drop.
struct ConsoleModeGuard {
    handle: HANDLE,
    original_mode: CONSOLE_MODE,
}

impl Drop for ConsoleModeGuard {
    fn drop(&mut self) {
        unsafe {
            SetConsoleMode(self.handle, self.original_mode);
        }
    }
}

impl ConsoleHandle {
    fn raw(&self) -> HANDLE {
        match self {
            ConsoleHandle::Borrowed(h) | ConsoleHandle::Owned(h) => *h,
        }
    }

    /// Change the console mode, returning a guard that restores the original on drop.
    fn set_mode(
        &self,
        f: impl FnOnce(CONSOLE_MODE) -> CONSOLE_MODE,
    ) -> miette::Result<ConsoleModeGuard> {
        let handle = self.raw();
        let mut original_mode: CONSOLE_MODE = 0;
        if unsafe { GetConsoleMode(handle, &mut original_mode) } == 0 {
            return Err(miette::miette!("GetConsoleMode failed on console handle"));
        }
        let mode = f(original_mode);
        if unsafe { SetConsoleMode(handle, mode) } == 0 {
            return Err(miette::miette!("SetConsoleMode failed"));
        }
        Ok(ConsoleModeGuard {
            handle,
            original_mode,
        })
    }

    /// Read a line from the console, handling backspace. Reads until Enter.
    fn read_line(&self, timeout_secs: u32) -> Result<String, ErrorCode> {
        let bytes = self.read_line_bytes(timeout_secs)?;
        String::from_utf8(bytes).map_err(|_| ErrorCode::GENERAL)
    }

    /// Read a line into raw bytes, handling backspace. Reads until Enter.
    ///
    /// Uses `ReadConsoleInputW` to read raw input records, filtering for
    /// actual key events. This avoids spurious wakeups from non-key events
    /// (mouse, focus, resize) that `WaitForSingleObject` on a console handle
    /// can produce, which would cause `ReadConsoleW` to block past the
    /// deadline.
    ///
    /// `ENABLE_PROCESSED_INPUT` is disabled so Ctrl+C arrives as a key event
    /// rather than terminating the process.
    fn read_line_bytes(&self, timeout_secs: u32) -> Result<Vec<u8>, ErrorCode> {
        let handle = self.raw();

        // Disable ENABLE_PROCESSED_INPUT so Ctrl+C is delivered as a key
        // event rather than a signal that terminates the process.
        let _mode_guard = self
            .set_mode(|m| m & !ENABLE_PROCESSED_INPUT)
            .map_err(|_| ErrorCode::GENERAL)?;

        let timeout = if timeout_secs > 0 {
            Some(std::time::Duration::from_secs(timeout_secs as u64))
        } else {
            None
        };
        let mut deadline = timeout.map(|t| Instant::now() + t);

        let mut buf = Vec::new();
        let mut pending_high: Option<u16> = None;
        loop {
            if let Some(d) = deadline {
                if Instant::now() >= d {
                    return Err(ErrorCode::TIMEOUT);
                }
                let remaining = d.saturating_duration_since(Instant::now());
                let ms = remaining.as_millis() as u32;
                match unsafe { WaitForSingleObject(handle, ms) } {
                    WAIT_OBJECT_0 => {}
                    WAIT_TIMEOUT => {
                        if Instant::now() >= d {
                            return Err(ErrorCode::TIMEOUT);
                        }
                        continue;
                    }
                    _ => return Err(ErrorCode::GENERAL),
                }
            }

            let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut events_read: u32 = 0;
            if unsafe { ReadConsoleInputW(handle, &mut record, 1, &mut events_read) } == 0 {
                return Err(ErrorCode::GENERAL);
            }
            if events_read == 0 {
                continue;
            }

            if record.EventType != KEY_EVENT as u16 {
                continue;
            }

            let key: KEY_EVENT_RECORD = unsafe { record.Event.KeyEvent };
            if key.bKeyDown == 0 {
                continue;
            }

            let c = unsafe { key.uChar.UnicodeChar };

            // Ctrl+C (ETX, U+0003)
            if c == 0x0003 {
                return Err(ErrorCode::CANCELED);
            }

            // A valid keypress resets the idle timeout.
            deadline = timeout.map(|t| Instant::now() + t);

            match c {
                0x000D | 0x000A => break,
                0x0008 | 0x007F => {
                    pending_high = None;
                    if !buf.is_empty() {
                        let mut i = buf.len() - 1;
                        while i > 0 && (buf[i] & 0xC0) == 0x80 {
                            i -= 1;
                        }
                        buf.truncate(i);
                    }
                }
                c if (0xD800..=0xDBFF).contains(&c) => {
                    pending_high = Some(c);
                }
                c if (0xDC00..=0xDFFF).contains(&c) => {
                    if let Some(high) = pending_high.take()
                        && let Some(ch) = char::decode_utf16([high, c]).next().and_then(|r| r.ok())
                    {
                        let mut tmp = [0u8; 4];
                        buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                    }
                }
                c => {
                    pending_high = None;
                    if c != 0
                        && let Some(ch) = char::from_u32(c as u32)
                    {
                        let mut tmp = [0u8; 4];
                        buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
                    }
                }
            }
        }
        Ok(buf)
    }
}

impl Drop for ConsoleHandle {
    fn drop(&mut self) {
        if let ConsoleHandle::Owned(h) = self {
            unsafe {
                CloseHandle(*h);
            }
        }
    }
}

impl Write for ConsoleHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Convert UTF-8 bytes to UTF-16 and write via WriteConsoleW.
        let text = std::str::from_utf8(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut written: u32 = 0;
        if unsafe {
            WriteConsoleW(
                self.raw(),
                wide.as_ptr() as *const _,
                wide.len() as u32,
                &mut written,
                std::ptr::null(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(buf.len()) // return original byte count
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Parse a `/conhost/<pid>` ttyname value, returning the PID if valid.
fn parse_conhost_pid(ttyname: &str) -> Option<u32> {
    let pid_str = ttyname.strip_prefix("/conhost/")?;
    pid_str.parse::<u32>().ok().filter(|&pid| pid != 0)
}

/// Resolve which Windows console to use.
///
/// Three-tier fallback:
/// 1. **Direct** — stdin is already a real console (`GetConsoleMode` succeeds)
/// 2. **Ttyname** — `OPTION ttyname` provides `/conhost/<pid>`, attach via `AttachConsole`
/// 3. **Allocated** — `AllocConsole()` as last resort
fn resolve_console_source(state: &PinentryState) -> miette::Result<ConsoleSource> {
    // Step 1: Check if stdin is already a real console.
    let std_handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut mode: u32 = 0;
    if std_handle != INVALID_HANDLE_VALUE
        && !std_handle.is_null()
        && unsafe { GetConsoleMode(std_handle, &mut mode) } != 0
    {
        tracing::debug!("ConsoleSource: Direct (stdin is a console)");
        return Ok(ConsoleSource::Direct);
    }

    // Step 2: Check if OPTION ttyname provides a /conhost/<pid>.
    if let Some(ref ttyname) = state.ttyname
        && let Some(pid) = parse_conhost_pid(ttyname)
    {
        tracing::debug!("ConsoleSource: trying ttyname attach to PID {pid}");
        unsafe { FreeConsole() };
        let attached = unsafe { AttachConsole(pid) };
        if attached != 0 {
            tracing::debug!("ConsoleSource: Ttyname (attached to PID {pid})");
            return Ok(ConsoleSource::Ttyname { pid });
        }
        tracing::debug!("ConsoleSource: AttachConsole({pid}) failed, falling through");
    }

    // Step 3: Allocate a new console as last resort.
    unsafe { FreeConsole() };
    let allocated = unsafe { AllocConsole() };
    tracing::debug!("ConsoleSource: Allocated (AllocConsole returned {allocated})");
    Ok(ConsoleSource::Allocated)
}

/// Open a Windows console device. Returns `Some(handle)` on success.
fn open_device(device: &str, access: u32) -> Option<HANDLE> {
    let device_w: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe {
        CreateFileW(
            device_w.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            INVALID_HANDLE_VALUE,
        )
    };
    if h != INVALID_HANDLE_VALUE && !h.is_null() {
        Some(h)
    } else {
        None
    }
}

/// Open a console handle from a resolved `ConsoleSource`.
fn open_console_handle(
    source: &ConsoleSource,
    device: &str,
    access: u32,
) -> miette::Result<ConsoleHandle> {
    match source {
        ConsoleSource::Direct => {
            let std_handle = match device {
                "CONIN$" => unsafe { GetStdHandle(STD_INPUT_HANDLE) },
                _ => unsafe { GetStdHandle(STD_OUTPUT_HANDLE) },
            };
            Ok(ConsoleHandle::Borrowed(std_handle))
        }
        ConsoleSource::Ttyname { .. } | ConsoleSource::Allocated => open_device(device, access)
            .map(ConsoleHandle::Owned)
            .ok_or_else(|| miette::miette!("failed to open {device}")),
    }
}

/// Resolve console source and open both CONIN$ and CONOUT$ handles.
///
/// Returns the source alongside the handles so it stays alive for their
/// entire lifetime — `Drop` detaches the console, which must happen after
/// all console I/O is complete.
fn resolve_console_handles(
    state: &PinentryState,
) -> miette::Result<(ConsoleHandle, ConsoleHandle, ConsoleSource)> {
    let source = resolve_console_source(state)?;
    let writer = open_console_handle(&source, "CONOUT$", 0x40000000)?;
    let reader = open_console_handle(&source, "CONIN$", 0xC0000000)?;
    Ok((writer, reader, source))
}

/// Read a password with echo disabled. Returns `SecretBytes` (zeroed on drop).
fn read_password(
    reader: &ConsoleHandle,
    writer: &mut ConsoleHandle,
    timeout_secs: u32,
) -> Result<SecretBytes, ErrorCode> {
    let secret = SecretBytes::from(reader.read_line_bytes(timeout_secs)?);

    // Echo was disabled, so the user's Enter didn't produce a visible newline
    writeln!(writer).map_err(|_| ErrorCode::GENERAL)?;

    Ok(secret)
}

pub(super) fn get_pin(state: &PinentryState) -> Result<SecretBytes, ErrorCode> {
    let (mut writer, reader, _source) =
        resolve_console_handles(state).map_err(|_| ErrorCode::GENERAL)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").map_err(|_| ErrorCode::GENERAL)?;
    }
    if let Some(ref err) = state.error {
        writeln!(writer, "ERROR: {err}").map_err(|_| ErrorCode::GENERAL)?;
    }
    let prompt = &state.prompt;
    write!(writer, "{prompt} ").map_err(|_| ErrorCode::GENERAL)?;
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    let pin = read_password(&reader, &mut writer, state.timeout)?;

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
    write!(writer, "[OK] ").map_err(|_| ErrorCode::GENERAL)?;
    writer.flush().map_err(|_| ErrorCode::GENERAL)?;

    reader.read_line(state.timeout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_conhost_pid_valid() {
        assert_eq!(parse_conhost_pid("/conhost/1234"), Some(1234));
    }

    #[test]
    fn parse_conhost_pid_large() {
        assert_eq!(parse_conhost_pid("/conhost/4294967295"), Some(u32::MAX));
    }

    #[test]
    fn parse_conhost_pid_zero_returns_none() {
        assert_eq!(parse_conhost_pid("/conhost/0"), None);
    }

    #[test]
    fn parse_conhost_pid_overflow_returns_none() {
        assert_eq!(parse_conhost_pid("/conhost/4294967296"), None);
    }

    #[test]
    fn parse_conhost_pid_negative_returns_none() {
        assert_eq!(parse_conhost_pid("/conhost/-1"), None);
    }

    #[test]
    fn parse_conhost_pid_non_numeric_returns_none() {
        assert_eq!(parse_conhost_pid("/conhost/abc"), None);
    }

    #[test]
    fn parse_conhost_pid_empty_returns_none() {
        assert_eq!(parse_conhost_pid("/conhost/"), None);
    }

    #[test]
    fn parse_conhost_pid_wrong_prefix_returns_none() {
        assert_eq!(parse_conhost_pid("/dev/pts/0"), None);
    }

    #[test]
    fn parse_conhost_pid_empty_string_returns_none() {
        assert_eq!(parse_conhost_pid(""), None);
    }

    #[test]
    fn parse_conhost_pid_no_slash_prefix_returns_none() {
        assert_eq!(parse_conhost_pid("conhost/1234"), None);
    }
}
