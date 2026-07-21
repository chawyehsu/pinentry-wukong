use std::io::Write;

use miette::IntoDiagnostic;

use crate::state::{ConfirmResult, GetPinResult, PinentryState, SecretBytes};

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
        if let ConsoleSource::Ttyname { pid } = self {
            tracing::debug!("ConsoleSource: releasing ttyname console from PID {pid}");
            unsafe {
                windows_sys::Win32::System::Console::FreeConsole();
            }
        }
    }
}

/// RAII wrapper for Windows console handles.
///
/// Tracks whether the handle is borrowed (std handle — must NOT be closed)
/// or owned (from CreateFileW — must be closed on drop).
enum ConsoleHandle {
    /// Borrowed from GetStdHandle — do not close.
    Borrowed(windows_sys::Win32::Foundation::HANDLE),
    /// Owned — created via CreateFileW, must be closed.
    Owned(windows_sys::Win32::Foundation::HANDLE),
}

impl ConsoleHandle {
    fn raw(&self) -> windows_sys::Win32::Foundation::HANDLE {
        match self {
            ConsoleHandle::Borrowed(h) | ConsoleHandle::Owned(h) => *h,
        }
    }
}

impl Drop for ConsoleHandle {
    fn drop(&mut self) {
        if let ConsoleHandle::Owned(h) = self {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(*h);
            }
        }
    }
}

impl Write for ConsoleHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use windows_sys::Win32::System::Console::WriteConsoleW;

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
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE,
    };

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
    use windows_sys::Win32::System::Console::AllocConsole;
    unsafe { FreeConsole() };
    let allocated = unsafe { AllocConsole() };
    tracing::debug!("ConsoleSource: Allocated (AllocConsole returned {allocated})");
    Ok(ConsoleSource::Allocated)
}

/// Open a Windows console device. Returns `Some(handle)` on success.
fn open_device(device: &str, access: u32) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

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
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};

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
fn resolve_console_handles(
    state: &PinentryState,
) -> miette::Result<(ConsoleSource, ConsoleHandle, ConsoleHandle)> {
    let source = resolve_console_source(state)?;
    let writer = open_console_handle(&source, "CONOUT$", 0x40000000)?;
    let reader = open_console_handle(&source, "CONIN$", 0xC0000000)?;
    Ok((source, writer, reader))
}

/// Read a line from a console handle (Windows).
///
/// Uses ReadConsoleW to read wide characters until Enter.
fn read_line_from_console(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> miette::Result<String> {
    use windows_sys::Win32::System::Console::ReadConsoleW;

    let mut line = String::new();
    loop {
        let mut buf: [u16; 1] = [0];
        let mut chars_read: u32 = 0;
        if unsafe {
            ReadConsoleW(
                handle,
                buf.as_mut_ptr() as *mut _,
                1,
                &mut chars_read,
                std::ptr::null(),
            )
        } == 0
        {
            return Err(miette::miette!("ReadConsoleW failed"));
        }
        if chars_read == 0 {
            break;
        }
        match buf[0] {
            0x000D | 0x000A => break,
            c => {
                if let Some(ch) = char::from_u32(c as u32) {
                    line.push(ch);
                }
            }
        }
    }
    Ok(line)
}

/// Read a password from a Windows console handle with echo disabled.
///
/// Caller provides the console input handle (already opened).
/// Console mode is restored before returning.
fn read_password_windows(console_in: &ConsoleHandle) -> miette::Result<String> {
    use windows_sys::Win32::System::Console::{GetConsoleMode, ReadConsoleW, SetConsoleMode};

    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;

    let handle = console_in.raw();

    // Save original console mode
    let mut original_mode: u32 = 0;
    if unsafe { GetConsoleMode(handle, &mut original_mode) } == 0 {
        return Err(miette::miette!("GetConsoleMode failed on console handle"));
    }

    // Disable echo, keep line input and processed input
    let new_mode =
        (original_mode | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT) & !ENABLE_ECHO_INPUT;
    if unsafe { SetConsoleMode(handle, new_mode) } == 0 {
        return Err(miette::miette!(
            "failed to disable echo (SetConsoleMode failed)"
        ));
    }

    // Read characters until Enter
    let mut password = String::new();
    let result = (|| -> miette::Result<()> {
        loop {
            let mut buf: [u16; 1] = [0];
            let mut chars_read: u32 = 0;
            if unsafe {
                ReadConsoleW(
                    handle,
                    buf.as_mut_ptr() as *mut _,
                    1,
                    &mut chars_read,
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(miette::miette!("ReadConsoleW failed"));
            }
            if chars_read == 0 {
                break;
            }
            match buf[0] {
                0x000D | 0x000A => break,
                0x0008 | 0x007F => {
                    password.pop();
                }
                c => {
                    if let Some(ch) = char::from_u32(c as u32) {
                        password.push(ch);
                    }
                }
            }
        }
        Ok(())
    })();

    // Always restore original mode (before console is dropped/closed)
    unsafe {
        SetConsoleMode(handle, original_mode);
    }
    // console is dropped here — Owned handle is closed automatically

    result?;

    // Write a newline since echo was disabled
    let _ = writeln!(std::io::stdout());

    Ok(password)
}

pub(super) fn get_pin(state: &PinentryState) -> miette::Result<GetPinResult> {
    let (_source, mut writer, console_in) = resolve_console_handles(state)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").into_diagnostic()?;
    }
    if let Some(ref err) = state.error {
        writeln!(writer, "ERROR: {err}").into_diagnostic()?;
    }
    let prompt = &state.prompt;
    write!(writer, "{prompt} ").into_diagnostic()?;
    writer.flush().into_diagnostic()?;

    let pin = read_password_windows(&console_in)?;

    if pin.is_empty() {
        return Ok(GetPinResult::Closed);
    }
    Ok(GetPinResult::Pin(SecretBytes::from(pin.into_bytes())))
}

pub(super) fn confirm(state: &PinentryState) -> miette::Result<ConfirmResult> {
    let (_source, mut writer, console_in) = resolve_console_handles(state)?;

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

    let line = read_line_from_console(console_in.raw())?;

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
    let (_source, mut writer, console_in) = resolve_console_handles(state)?;

    if let Some(ref desc) = state.description {
        writeln!(writer, "{desc}").into_diagnostic()?;
    }
    write!(writer, "[OK] ").into_diagnostic()?;
    writer.flush().into_diagnostic()?;

    read_line_from_console(console_in.raw())?;
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
