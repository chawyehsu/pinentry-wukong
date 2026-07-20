use std::io::{self, Read, Write};
use std::os::unix::io::RawFd;

use super::PinentryServer;

/// Raw fd reader that reads directly from a file descriptor.
/// This avoids holding `StdinLock`, which would block crossterm's event system.
struct FdReader {
    fd: RawFd,
}

impl FdReader {
    fn new(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl Read for FdReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}

/// Raw fd writer that writes directly to a file descriptor.
/// This bypasses any stdio buffering and is immune to fd redirections.
struct FdWriter {
    fd: RawFd,
}

impl FdWriter {
    fn new(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl Write for FdWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(self.fd, buf.as_ptr() as *const _, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // raw fd writes are not buffered
    }
}

/// Run the pinentry server on stdin/stdout.
///
/// Saves the original stdout pipe fd before any TTY redirection happens,
/// and writes Assuan responses directly to that fd. This avoids issues with
/// fd redirection by the TUI (which redirects stdin/stdout to the terminal).
pub fn start(
    ui: &dyn crate::ui::PinentryUi,
    grab: bool,
    timeout: u32,
    keychain: Option<Box<dyn crate::keychain::Keychain>>,
) -> miette::Result<()> {
    // Save the original stdout fd (the Assuan pipe) before any redirection.
    // We write Assuan responses to this fd directly, so the TUI's fd
    // redirection doesn't affect protocol output.
    let pipe_stdout = unsafe { libc::dup(libc::STDOUT_FILENO) };
    if pipe_stdout < 0 {
        return Err(miette::miette!("failed to save stdout fd"));
    }

    // Dup stdin so we don't hold StdinLock — crossterm's event system
    // needs unlocked access to fd 0 for TUI key reading.
    let pipe_stdin = unsafe { libc::dup(libc::STDIN_FILENO) };
    if pipe_stdin < 0 {
        unsafe { libc::close(pipe_stdout) };
        return Err(miette::miette!("failed to dup stdin fd"));
    }

    let reader = FdReader::new(pipe_stdin);
    let writer = FdWriter::new(pipe_stdout);

    let mut server = PinentryServer::new(reader, writer, grab, timeout, keychain);
    let result = server.run(ui).map_err(|e| miette::miette!("{e}"));

    unsafe {
        libc::close(pipe_stdin);
        libc::close(pipe_stdout);
    }
    result
}
