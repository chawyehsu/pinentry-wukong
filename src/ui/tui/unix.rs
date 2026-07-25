use std::time::Duration;

use crossterm::terminal::disable_raw_mode;
use miette::IntoDiagnostic;

use super::Key;
use crate::state::PinentryState;

pub(super) struct TtyGuard {
    saved_stdin: std::os::unix::io::RawFd,
    saved_stdout: std::os::unix::io::RawFd,
    tty_fd: std::os::unix::io::RawFd,
}

impl TtyGuard {
    pub(super) fn redirect(state: &PinentryState) -> miette::Result<Self> {
        let path = state
            .ttyname
            .clone()
            .unwrap_or_else(|| "/dev/tty".to_string());
        tracing::debug!("TUI: redirecting to terminal: {path}");

        let path_cstr = std::ffi::CString::new(path.clone())
            .map_err(|_| miette::miette!("invalid tty path: {path}"))?;

        let saved_stdin = unsafe { libc::dup(libc::STDIN_FILENO) };
        if saved_stdin < 0 {
            return Err(miette::miette!(
                "failed to dup stdin: {}",
                std::io::Error::last_os_error()
            ));
        }
        let saved_stdout = unsafe { libc::dup(libc::STDOUT_FILENO) };
        if saved_stdout < 0 {
            unsafe {
                libc::close(saved_stdin);
            }
            return Err(miette::miette!(
                "failed to dup stdout: {}",
                std::io::Error::last_os_error()
            ));
        }

        let tty_fd = unsafe { libc::open(path_cstr.as_ptr(), libc::O_RDWR) };
        if tty_fd < 0 {
            unsafe {
                libc::close(saved_stdin);
                libc::close(saved_stdout);
            }
            return Err(miette::miette!(
                "failed to open terminal {path}: {}",
                std::io::Error::last_os_error()
            ));
        }

        if unsafe { libc::dup2(tty_fd, libc::STDIN_FILENO) } < 0
            || unsafe { libc::dup2(tty_fd, libc::STDOUT_FILENO) } < 0
        {
            unsafe {
                libc::close(tty_fd);
                libc::dup2(saved_stdin, libc::STDIN_FILENO);
                libc::dup2(saved_stdout, libc::STDOUT_FILENO);
                libc::close(saved_stdin);
                libc::close(saved_stdout);
            }
            return Err(miette::miette!("failed to redirect fds to tty"));
        }

        tracing::debug!("TUI: stdin/stdout redirected to {path} (tty_fd={tty_fd})");
        Ok(Self {
            saved_stdin,
            saved_stdout,
            tty_fd,
        })
    }

    pub(super) fn handle(&self) -> std::os::unix::io::RawFd {
        self.tty_fd
    }
}

impl Drop for TtyGuard {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.tty_fd);
            libc::dup2(self.saved_stdin, libc::STDIN_FILENO);
            libc::dup2(self.saved_stdout, libc::STDOUT_FILENO);
            libc::close(self.saved_stdin);
            libc::close(self.saved_stdout);
        }
        tracing::debug!("TUI: restored stdin/stdout to Assuan pipes");
    }
}

pub(super) fn cleanup_terminal() -> miette::Result<()> {
    disable_raw_mode().into_diagnostic()?;
    // On Unix, write escape sequences directly to the TTY to ensure they
    // reach the terminal even if stdout has been redirected.
    use std::io::Write;
    let mut tty = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .into_diagnostic()?;
    tty.write_all(b"\x1b[?1049l").ok(); // leave alternate screen
    tty.write_all(b"\x1b[?25h").ok(); // show cursor
    tty.write_all(b"\x1b[r").ok(); // reset scroll region
    tty.write_all(b"\x1b[999;1H").ok(); // move cursor to bottom
    tty.write_all(b"\n").ok();
    Ok(())
}

fn read_key(tty_fd: std::os::unix::io::RawFd) -> Option<Key> {
    let mut byte = [0u8; 1];
    loop {
        let n = unsafe { libc::read(tty_fd, byte.as_mut_ptr() as *mut _, 1) };
        if n <= 0 {
            return None;
        }
        match byte[0] {
            b'\r' | b'\n' => return Some(Key::Enter),
            0x7f | 0x08 => return Some(Key::Backspace),
            0x03 => return Some(Key::CtrlC),
            0x09 => return Some(Key::Tab),
            0x1b => {
                let mut seq = [0u8; 2];
                let ready = unsafe {
                    let mut seqfds: libc::fd_set = std::mem::zeroed();
                    libc::FD_ZERO(&mut seqfds);
                    libc::FD_SET(tty_fd, &mut seqfds);
                    let mut seq_tv = libc::timeval {
                        tv_sec: 0,
                        tv_usec: 50_000,
                    };
                    let r = libc::select(
                        tty_fd + 1,
                        &mut seqfds,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut seq_tv,
                    );
                    r > 0 && libc::FD_ISSET(tty_fd, &seqfds)
                };
                if ready {
                    let n2 = unsafe { libc::read(tty_fd, seq.as_mut_ptr() as *mut _, 2) };
                    if n2 >= 2 && seq[0] == b'[' {
                        match seq[1] {
                            b'D' => return Some(Key::Left),
                            b'C' => return Some(Key::Right),
                            b'Z' => return Some(Key::BackTab),
                            _ => {}
                        }
                    }
                }
                return Some(Key::Esc);
            }
            c if (0x20..=0x7e).contains(&c) => return Some(Key::Char(c as char)),
            _ => {}
        }
    }
}

pub(super) fn poll_key(tty_fd: std::os::unix::io::RawFd, timeout: Duration) -> Option<Key> {
    unsafe {
        let mut readfds: libc::fd_set = std::mem::zeroed();
        libc::FD_ZERO(&mut readfds);
        libc::FD_SET(tty_fd, &mut readfds);

        let mut timeval = libc::timeval {
            tv_sec: timeout.as_secs() as _,
            tv_usec: timeout.subsec_micros() as _,
        };

        let ready = libc::select(
            tty_fd + 1,
            &mut readfds,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut timeval,
        );

        if ready > 0 && libc::FD_ISSET(tty_fd, &readfds) {
            read_key(tty_fd)
        } else {
            None
        }
    }
}
