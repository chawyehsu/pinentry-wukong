#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::io::BufRead;
#[cfg(unix)]
use std::io::BufReader;
use std::io::Write;

use miette::IntoDiagnostic;

use crate::state::{ConfirmResult, GetPinResult, PinentryState, SecretBytes};
use crate::ui::PinentryUi;

/// Simple line-based TTY fallback UI.
pub struct TtyUi;

impl TtyUi {
    pub fn new() -> Self {
        Self
    }
}

// ── Unix helpers ──────────────────────────────────────────────────────────────

#[cfg(unix)]
fn tty_path(state: &PinentryState) -> String {
    state
        .ttyname
        .clone()
        .unwrap_or_else(|| "/dev/tty".to_string())
}

#[cfg(unix)]
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

// ── Windows helpers ───────────────────────────────────────────────────────────

/// How the Windows console was obtained.
#[cfg(windows)]
enum ConsoleSource {
    /// stdin is already a real console (`GetConsoleMode` succeeds).
    Direct,
    /// Attached to an ancestor process's console via process-tree walk.
    Inherited { pid: u32 },
    /// `AllocConsole()` created a new console window (last resort).
    Allocated,
}

#[cfg(windows)]
impl Drop for ConsoleSource {
    fn drop(&mut self) {
        if let ConsoleSource::Inherited { pid } = self {
            tracing::debug!("ConsoleSource: releasing inherited console from PID {pid}");
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
#[cfg(windows)]
enum ConsoleHandle {
    /// Borrowed from GetStdHandle — do not close.
    Borrowed(windows_sys::Win32::Foundation::HANDLE),
    /// Owned — created via CreateFileW, must be closed.
    Owned(windows_sys::Win32::Foundation::HANDLE),
}

#[cfg(windows)]
impl ConsoleHandle {
    fn raw(&self) -> windows_sys::Win32::Foundation::HANDLE {
        match self {
            ConsoleHandle::Borrowed(h) | ConsoleHandle::Owned(h) => *h,
        }
    }
}

#[cfg(windows)]
impl Drop for ConsoleHandle {
    fn drop(&mut self) {
        if let ConsoleHandle::Owned(h) = self {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(*h);
            }
        }
    }
}

#[cfg(windows)]
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

/// Resolve which Windows console to use.
///
/// Three-tier fallback:
/// 1. **Direct** — stdin is already a real console (`GetConsoleMode` succeeds)
/// 2. **Inherited** — walk the process tree to find an ancestor with a console
/// 3. **Allocated** — `AllocConsole()` as last resort
#[cfg(windows)]
fn resolve_console_source() -> miette::Result<ConsoleSource> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE,
    };

    const MAX_ANCESTOR_DEPTH: u32 = 10;

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

    // Step 2: Walk the process tree to find an ancestor with a console.
    // Use NtQueryInformationProcess to get the parent PID at each level.
    let mut current_pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
    for depth in 0..MAX_ANCESTOR_DEPTH {
        let parent_pid = match get_parent_pid(current_pid) {
            Some(pid) if pid != 0 => pid,
            _ => {
                tracing::debug!("ConsoleSource: no more ancestors at depth {depth}");
                break;
            }
        };

        tracing::debug!("ConsoleSource: trying ancestor PID {parent_pid} at depth {depth}");

        // Detach from any current console before attaching to the ancestor.
        unsafe { FreeConsole() };
        let attached = unsafe { AttachConsole(parent_pid) };
        if attached == 0 {
            tracing::debug!("ConsoleSource: AttachConsole({parent_pid}) failed, skipping");
            current_pid = parent_pid;
            continue;
        }

        // Verify we can actually open a console device.
        let test = open_device("CONIN$", 0xC0000000);
        if test.is_some() {
            tracing::debug!("ConsoleSource: Inherited from PID {parent_pid} at depth {depth}");
            return Ok(ConsoleSource::Inherited { pid: parent_pid });
        }

        current_pid = parent_pid;
    }

    // Step 3: Allocate a new console as last resort.
    use windows_sys::Win32::System::Console::AllocConsole;
    unsafe { FreeConsole() };
    let allocated = unsafe { AllocConsole() };
    tracing::debug!("ConsoleSource: Allocated (AllocConsole returned {allocated})");
    Ok(ConsoleSource::Allocated)
}

/// Get the parent PID of a process via `NtQueryInformationProcess`.
#[cfg(windows)]
fn get_parent_pid(pid: u32) -> Option<u32> {
    use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_BASIC_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // Open the target process.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return None;
    }

    let mut pbi: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let mut return_len: u32 = 0;
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            0, // ProcessBasicInformation
            &mut pbi as *mut _ as *mut _,
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &mut return_len,
        )
    };
    unsafe { CloseHandle(handle) };

    if status != 0 {
        return None;
    }

    Some(pbi.InheritedFromUniqueProcessId as u32)
}

/// Try to open a Windows console device. Returns `Some(handle)` on success.
#[cfg(windows)]
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
#[cfg(windows)]
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
        ConsoleSource::Inherited { .. } | ConsoleSource::Allocated => open_device(device, access)
            .map(ConsoleHandle::Owned)
            .ok_or_else(|| miette::miette!("failed to open {device}")),
    }
}

/// Resolve console source and open both CONIN$ and CONOUT$ handles.
#[cfg(windows)]
fn resolve_console_handles() -> miette::Result<(ConsoleSource, ConsoleHandle, ConsoleHandle)> {
    let source = resolve_console_source()?;
    let writer = open_console_handle(&source, "CONOUT$", 0x40000000)?;
    let reader = open_console_handle(&source, "CONIN$", 0xC0000000)?;
    Ok((source, writer, reader))
}

/// Read a line from a console handle (Windows).
///
/// Uses ReadConsoleW to read wide characters until Enter.
#[cfg(windows)]
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
#[cfg(windows)]
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

// ── PinentryUi impl ──────────────────────────────────────────────────────────

impl PinentryUi for TtyUi {
    fn flavor(&self) -> &str {
        "wukong:tty"
    }

    fn get_pin(&self, state: &PinentryState) -> miette::Result<GetPinResult> {
        #[cfg(unix)]
        let mut writer = {
            let path = tty_path(state);
            OpenOptions::new()
                .write(true)
                .open(&path)
                .into_diagnostic()?
        };
        #[cfg(windows)]
        let (mut writer, console_in) = {
            let (_source, w, r) = resolve_console_handles()?;
            (w, r)
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

        #[cfg(unix)]
        let pin = {
            let path = tty_path(state);
            let config = rpassword::ConfigBuilder::new()
                .input_file_path(&path)
                .output_file_path(&path)
                .build();
            rpassword::read_password_with_config(config).into_diagnostic()?
        };
        #[cfg(windows)]
        let pin = read_password_windows(&console_in)?;

        if pin.is_empty() {
            return Ok(GetPinResult::Closed);
        }
        Ok(GetPinResult::Pin(SecretBytes::from(pin.into_bytes())))
    }

    fn confirm(&self, state: &PinentryState) -> miette::Result<ConfirmResult> {
        #[cfg(unix)]
        let (mut reader, mut writer) = open_tty(state)?;
        #[cfg(windows)]
        let (mut writer, console_in) = {
            let (_source, w, r) = resolve_console_handles()?;
            (w, r)
        };

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

        #[cfg(windows)]
        let line = read_line_from_console(console_in.raw())?;
        #[cfg(unix)]
        let line = {
            let mut l = String::new();
            reader.read_line(&mut l).into_diagnostic()?;
            l
        };

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

    fn message(&self, state: &PinentryState) -> miette::Result<()> {
        #[cfg(unix)]
        let (mut reader, mut writer) = open_tty(state)?;
        #[cfg(windows)]
        let (mut writer, console_in) = {
            let (_source, w, r) = resolve_console_handles()?;
            (w, r)
        };

        if let Some(ref desc) = state.description {
            writeln!(writer, "{desc}").into_diagnostic()?;
        }
        write!(writer, "[OK] ").into_diagnostic()?;
        writer.flush().into_diagnostic()?;

        #[cfg(windows)]
        {
            read_line_from_console(console_in.raw())?;
        }
        #[cfg(unix)]
        {
            let mut line = String::new();
            reader.read_line(&mut line).into_diagnostic()?;
        }
        Ok(())
    }
}
