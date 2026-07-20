use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
#[cfg(windows)]
use std::os::windows::io::FromRawHandle;

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

/// Open a console handle on Windows.
///
/// When stdin/stdout are piped (e.g. gpg-agent spawning us), GetStdHandle
/// returns the pipe handle, not the console. We try GetStdHandle first,
/// then fall back to AttachConsole + CreateFileW.
#[cfg(windows)]
fn open_console_handle(
    std_handle: windows_sys::Win32::Foundation::HANDLE,
    device: &str,
    access: u32,
) -> miette::Result<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetConsoleMode,
    };

    // Check if the std handle is a real console (not a pipe)
    let mut mode: u32 = 0;
    if std_handle != INVALID_HANDLE_VALUE
        && !std_handle.is_null()
        && unsafe { GetConsoleMode(std_handle, &mut mode) } != 0
    {
        return Ok(std_handle);
    }

    // Handle is piped — try to attach to parent's console.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    tracing::debug!("AttachConsole({device}): {attached}");

    // Try to open the device directly.
    let device_w: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
    let try_open = || unsafe {
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

    let h = try_open();
    if h != INVALID_HANDLE_VALUE && !h.is_null() {
        return Ok(h);
    }

    // Open failed — allocate a console and try again.
    use windows_sys::Win32::System::Console::AllocConsole;
    let allocated = unsafe { AllocConsole() };
    tracing::debug!("AllocConsole({device}): {allocated}");
    let h = try_open();
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        return Err(miette::miette!("failed to open {device}"));
    }
    Ok(h)
}

/// Open the console for writing (Windows).
#[cfg(windows)]
fn open_console_out() -> miette::Result<File> {
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};

    let std_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let handle = open_console_handle(std_handle, "CONOUT$", 0x40000000)?; // GENERIC_WRITE
    Ok(unsafe { File::from_raw_handle(handle as *mut _) })
}

/// Read a password from the Windows console with echo disabled.
///
/// Opens CONIN$ via AttachConsole when stdin is piped, then uses
/// SetConsoleMode + ReadConsoleW to read with echo disabled.
#[cfg(windows)]
fn read_password_windows() -> miette::Result<String> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, ReadConsoleW, STD_INPUT_HANDLE, SetConsoleMode,
    };

    const ENABLE_ECHO_INPUT: u32 = 0x0004;
    const ENABLE_LINE_INPUT: u32 = 0x0002;
    const ENABLE_PROCESSED_INPUT: u32 = 0x0001;

    let std_handle: HANDLE = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let handle = open_console_handle(std_handle, "CONIN$", 0xC0000000)?; // GENERIC_READ | GENERIC_WRITE

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
                0x000D | 0x000A => break, // \r or \n
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

    // Always restore original mode
    unsafe {
        SetConsoleMode(handle, original_mode);
    }

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
        let mut writer = open_console_out()?;

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
        let pin = read_password_windows()?;

        if pin.is_empty() {
            return Ok(GetPinResult::Closed);
        }
        Ok(GetPinResult::Pin(SecretBytes::from(pin.into_bytes())))
    }

    fn confirm(&self, state: &PinentryState) -> miette::Result<ConfirmResult> {
        #[cfg(unix)]
        let (mut reader, mut writer) = open_tty(state)?;
        #[cfg(windows)]
        let (mut reader, mut writer) = {
            let w = open_console_out()?;
            let r = BufReader::new(
                OpenOptions::new()
                    .read(true)
                    .open("CONIN$")
                    .or_else(|_| OpenOptions::new().read(true).open("CON"))
                    .into_diagnostic()?,
            );
            (r, w)
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
        #[cfg(unix)]
        let (mut reader, mut writer) = open_tty(state)?;
        #[cfg(windows)]
        let (mut reader, mut writer) = {
            let w = open_console_out()?;
            let r = BufReader::new(
                OpenOptions::new()
                    .read(true)
                    .open("CONIN$")
                    .or_else(|_| OpenOptions::new().read(true).open("CON"))
                    .into_diagnostic()?,
            );
            (r, w)
        };

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
