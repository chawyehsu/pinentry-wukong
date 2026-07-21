use std::time::{Duration, Instant};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use miette::IntoDiagnostic;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use crate::state::{ConfirmResult, GetPinResult, PinentryState, SecretBytes};
use crate::ui::PinentryUi;

#[cfg(unix)]
type TtyHandle = std::os::unix::io::RawFd;
#[cfg(windows)]
type TtyHandle = (); // Windows uses crossterm events, no raw fd needed

type TuiTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

pub struct TuiUi;

impl TuiUi {
    pub fn new() -> Self {
        Self
    }
}

impl PinentryUi for TuiUi {
    fn flavor(&self) -> &str {
        "wukong:tui"
    }

    fn get_pin(&self, state: &PinentryState) -> miette::Result<GetPinResult> {
        tracing::debug!("TUI: get_pin called");
        let guard = TtyGuard::redirect(state)?;
        tracing::debug!("TUI: TtyGuard created, enabling raw mode");
        let mut terminal = create_terminal()?;
        #[allow(clippy::let_unit_value)]
        let handle = guard.handle();
        tracing::debug!("TUI: terminal created, entering get_pin loop");
        let result = run_getpin(&mut terminal, handle, state);
        tracing::debug!("TUI: get_pin loop exited with result: {:?}", result.is_ok());
        cleanup_terminal()?;
        drop(terminal);
        drop(guard);
        result
    }

    fn confirm(&self, state: &PinentryState) -> miette::Result<ConfirmResult> {
        let guard = TtyGuard::redirect(state)?;
        let mut terminal = create_terminal()?;
        #[allow(clippy::let_unit_value)]
        let handle = guard.handle();
        let result = run_confirm(&mut terminal, handle, state);
        cleanup_terminal()?;
        drop(terminal);
        drop(guard);
        result
    }

    fn message(&self, state: &PinentryState) -> miette::Result<()> {
        let guard = TtyGuard::redirect(state)?;
        let mut terminal = create_terminal()?;
        #[allow(clippy::let_unit_value)]
        let handle = guard.handle();
        let result = run_message(&mut terminal, handle, state);
        cleanup_terminal()?;
        drop(terminal);
        drop(guard);
        result
    }
}

// -- Platform-specific TtyGuard --

#[cfg(unix)]
struct TtyGuard {
    saved_stdin: std::os::unix::io::RawFd,
    saved_stdout: std::os::unix::io::RawFd,
    tty_fd: std::os::unix::io::RawFd,
}

#[cfg(unix)]
impl TtyGuard {
    fn redirect(state: &PinentryState) -> miette::Result<Self> {
        use std::os::unix::io::RawFd;

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

    fn handle(&self) -> TtyHandle {
        self.tty_fd
    }
}

#[cfg(unix)]
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

#[cfg(windows)]
struct TtyGuard;

#[cfg(windows)]
impl TtyGuard {
    fn redirect(_state: &PinentryState) -> miette::Result<Self> {
        // On Windows, crossterm uses ReadConsoleInputW/WriteConsoleOutputW
        // which don't conflict with stdin/stdout fd redirections.
        // No fd manipulation needed — stdin/stdout work directly.
        tracing::debug!("TUI: Windows — no fd redirection needed");
        Ok(Self)
    }

    fn handle(&self) -> TtyHandle {}
}

#[cfg(windows)]
impl Drop for TtyGuard {
    fn drop(&mut self) {
        tracing::debug!("TUI: Windows — no fd restoration needed");
    }
}

// -- Terminal setup and cleanup --

fn create_terminal() -> miette::Result<TuiTerminal> {
    enable_raw_mode().into_diagnostic()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).into_diagnostic()?;
    terminal.clear().into_diagnostic()?;
    Ok(terminal)
}

#[cfg(unix)]
fn cleanup_terminal() -> miette::Result<()> {
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

#[cfg(windows)]
fn cleanup_terminal() -> miette::Result<()> {
    disable_raw_mode().into_diagnostic()?;
    // On Windows, crossterm handles terminal cleanup via the Console API.
    // The escape sequences are processed by the Windows terminal directly.
    Ok(())
}

// -- Key input --

/// A parsed key event from terminal input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    CtrlC,
    Left,
    Right,
}

#[cfg(unix)]
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

#[cfg(unix)]
fn poll_key(tty_fd: std::os::unix::io::RawFd, timeout: Duration) -> Option<Key> {
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

#[cfg(windows)]
fn crossterm_event_to_key(event: crossterm::event::Event) -> Option<Key> {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    match event {
        Event::Key(KeyEvent {
            code, modifiers, ..
        }) => match code {
            KeyCode::Enter => Some(Key::Enter),
            KeyCode::Esc => Some(Key::Esc),
            KeyCode::Backspace => Some(Key::Backspace),
            KeyCode::Tab => Some(Key::Tab),
            KeyCode::Left => Some(Key::Left),
            KeyCode::Right => Some(Key::Right),
            KeyCode::Char(c) => {
                if modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                    Some(Key::CtrlC)
                } else {
                    Some(Key::Char(c))
                }
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(windows)]
#[allow(dead_code)]
fn read_key(_handle: TtyHandle) -> Option<Key> {
    use crossterm::event;
    match event::read().ok() {
        Some(ev) => crossterm_event_to_key(ev),
        None => None,
    }
}

#[cfg(windows)]
fn poll_key(_handle: TtyHandle, timeout: Duration) -> Option<Key> {
    use crossterm::event;
    if event::poll(timeout).ok() == Some(true) {
        match event::read().ok() {
            Some(ev) => {
                // Handle Shift+Tab (BackTab) — crossterm on Windows sends
                // KeyCode::BackTab directly, but let's also handle the
                // modifier variant.
                use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
                match &ev {
                    Event::Key(KeyEvent {
                        code: KeyCode::Tab,
                        modifiers,
                        ..
                    }) if modifiers.contains(KeyModifiers::SHIFT) => Some(Key::BackTab),
                    Event::Key(KeyEvent {
                        code: KeyCode::BackTab,
                        ..
                    }) => Some(Key::BackTab),
                    _ => crossterm_event_to_key(ev),
                }
            }
            None => None,
        }
    } else {
        None
    }
}

// -- Focus state --

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GetPinFocus {
    Input,
    Ok,
    Cancel,
}

impl GetPinFocus {
    fn next(self) -> Self {
        match self {
            GetPinFocus::Input => GetPinFocus::Ok,
            GetPinFocus::Ok => GetPinFocus::Cancel,
            GetPinFocus::Cancel => GetPinFocus::Input,
        }
    }
    fn prev(self) -> Self {
        match self {
            GetPinFocus::Input => GetPinFocus::Cancel,
            GetPinFocus::Ok => GetPinFocus::Input,
            GetPinFocus::Cancel => GetPinFocus::Ok,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmFocus {
    Ok,
    NotOk,
    Cancel,
}

impl ConfirmFocus {
    fn next(self, has_notok: bool) -> Self {
        match self {
            ConfirmFocus::Ok => {
                if has_notok {
                    ConfirmFocus::NotOk
                } else {
                    ConfirmFocus::Cancel
                }
            }
            ConfirmFocus::NotOk => ConfirmFocus::Cancel,
            ConfirmFocus::Cancel => ConfirmFocus::Ok,
        }
    }
}

// -- GETPIN --

fn run_getpin(
    terminal: &mut TuiTerminal,
    handle: TtyHandle,
    state: &PinentryState,
) -> miette::Result<GetPinResult> {
    let title = state.title.as_deref().unwrap_or(env!("CARGO_PKG_NAME"));
    let description = state.description.as_deref().unwrap_or("");
    let prompt = &state.prompt;
    let error = state.error.as_deref();
    let ok_label = &state.ok;
    let cancel_label = &state.cancel;

    let mut input = String::new();
    let mut focus = GetPinFocus::Input;
    let start = Instant::now();
    let timeout = if state.timeout > 0 {
        Some(Duration::from_secs(state.timeout as u64))
    } else {
        None
    };

    loop {
        if let Some(t) = timeout
            && start.elapsed() >= t
        {
            return Ok(GetPinResult::Canceled);
        }

        tracing::trace!("TUI: drawing frame");
        terminal
            .draw(|f| {
                let area = centered_rect(60, 12, f.area());
                f.render_widget(Clear, area);

                let block = Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::horizontal(1));
                let inner = block.inner(area);
                f.render_widget(block, area);

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(desc_lines(description, inner.width) as u16),
                        Constraint::Length(if error.is_some() { 2 } else { 0 }),
                        Constraint::Length(2),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(inner);

                if !description.is_empty() {
                    f.render_widget(
                        Paragraph::new(description).style(Style::default().fg(Color::White)),
                        chunks[0],
                    );
                }
                if let Some(err) = error {
                    f.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            err,
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ))),
                        chunks[1],
                    );
                }

                let masked = "•".repeat(input.chars().count());
                let input_style = if focus == GetPinFocus::Input {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!("{prompt} "), Style::default().fg(Color::Yellow)),
                        Span::styled(masked, input_style),
                        Span::styled("▎", Style::default().fg(Color::White)),
                    ])),
                    chunks[2],
                );

                let ok_s = btn_style(focus == GetPinFocus::Ok);
                let cancel_s = btn_style(focus == GetPinFocus::Cancel);
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!(" [ {ok_label} ] "), ok_s),
                        Span::raw("  "),
                        Span::styled(format!(" [ {cancel_label} ] "), cancel_s),
                    ])),
                    chunks[4],
                );
            })
            .into_diagnostic()?;

        let poll = timeout
            .map(|t| t.saturating_sub(start.elapsed()))
            .unwrap_or(Duration::from_millis(100));

        if let Some(key) = poll_key(handle, poll) {
            match handle_getpin_key(key, &mut focus, &mut input) {
                GetPinAction::Continue => {}
                GetPinAction::Submit => {
                    return Ok(GetPinResult::Pin(SecretBytes::from(input.into_bytes())));
                }
                GetPinAction::Cancel => return Ok(GetPinResult::Canceled),
            }
        }
    }
}

enum GetPinAction {
    Continue,
    Submit,
    Cancel,
}

fn handle_getpin_key(key: Key, focus: &mut GetPinFocus, input: &mut String) -> GetPinAction {
    match key {
        Key::CtrlC => GetPinAction::Cancel,
        Key::Esc => GetPinAction::Cancel,
        _ => match *focus {
            GetPinFocus::Input => match key {
                Key::Enter => GetPinAction::Submit,
                Key::Tab => {
                    *focus = focus.next();
                    GetPinAction::Continue
                }
                Key::BackTab => {
                    *focus = focus.prev();
                    GetPinAction::Continue
                }
                Key::Backspace => {
                    input.pop();
                    GetPinAction::Continue
                }
                Key::Char(c) => {
                    input.push(c);
                    GetPinAction::Continue
                }
                _ => GetPinAction::Continue,
            },
            GetPinFocus::Ok => match key {
                Key::Enter | Key::Char(' ') => GetPinAction::Submit,
                Key::Tab | Key::Right => {
                    *focus = focus.next();
                    GetPinAction::Continue
                }
                Key::BackTab | Key::Left => {
                    *focus = focus.prev();
                    GetPinAction::Continue
                }
                _ => GetPinAction::Continue,
            },
            GetPinFocus::Cancel => match key {
                Key::Enter | Key::Char(' ') => GetPinAction::Cancel,
                Key::Tab | Key::Right => {
                    *focus = focus.next();
                    GetPinAction::Continue
                }
                Key::BackTab | Key::Left => {
                    *focus = focus.prev();
                    GetPinAction::Continue
                }
                _ => GetPinAction::Continue,
            },
        },
    }
}

// -- CONFIRM --

fn run_confirm(
    terminal: &mut TuiTerminal,
    handle: TtyHandle,
    state: &PinentryState,
) -> miette::Result<ConfirmResult> {
    let title = state.title.as_deref().unwrap_or(env!("CARGO_PKG_NAME"));
    let description = state.description.as_deref().unwrap_or("");
    let error = state.error.as_deref();
    let ok_label = &state.ok;
    let cancel_label = &state.cancel;
    let has_notok = state.notok.is_some();
    let notok_label = state.notok.as_deref().unwrap_or("Not OK");

    let mut focus = ConfirmFocus::Ok;
    let start = Instant::now();
    let timeout = if state.timeout > 0 {
        Some(Duration::from_secs(state.timeout as u64))
    } else {
        None
    };

    loop {
        if let Some(t) = timeout
            && start.elapsed() >= t
        {
            return Ok(ConfirmResult::Canceled);
        }

        terminal
            .draw(|f| {
                let area = centered_rect(60, 10, f.area());
                f.render_widget(Clear, area);
                let block = Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::horizontal(1));
                let inner = block.inner(area);
                f.render_widget(block, area);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(desc_lines(description, inner.width) as u16),
                        Constraint::Length(if error.is_some() { 2 } else { 0 }),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(inner);
                if !description.is_empty() {
                    f.render_widget(
                        Paragraph::new(description).style(Style::default().fg(Color::White)),
                        chunks[0],
                    );
                }
                if let Some(err) = error {
                    f.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            err,
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        ))),
                        chunks[1],
                    );
                }
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(
                        format!(" [ {ok_label} ] "),
                        btn_style(focus == ConfirmFocus::Ok),
                    ),
                ];
                if has_notok {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        format!(" [ {notok_label} ] "),
                        btn_style(focus == ConfirmFocus::NotOk),
                    ));
                }
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!(" [ {cancel_label} ] "),
                    btn_style(focus == ConfirmFocus::Cancel),
                ));
                f.render_widget(Paragraph::new(Line::from(spans)), chunks[3]);
            })
            .into_diagnostic()?;

        let poll = timeout
            .map(|t| t.saturating_sub(start.elapsed()))
            .unwrap_or(Duration::from_millis(100));
        if let Some(key) = poll_key(handle, poll) {
            if key == Key::CtrlC || key == Key::Esc {
                return Ok(ConfirmResult::Canceled);
            }
            match focus {
                ConfirmFocus::Ok => match key {
                    Key::Enter | Key::Char(' ') => return Ok(ConfirmResult::Accepted),
                    Key::Tab | Key::Right => focus = focus.next(has_notok),
                    Key::BackTab | Key::Left => focus = ConfirmFocus::Cancel,
                    _ => {}
                },
                ConfirmFocus::NotOk => match key {
                    Key::Enter | Key::Char(' ') => return Ok(ConfirmResult::NotOk),
                    Key::Tab | Key::Right => focus = focus.next(has_notok),
                    Key::BackTab | Key::Left => focus = ConfirmFocus::Ok,
                    _ => {}
                },
                ConfirmFocus::Cancel => match key {
                    Key::Enter | Key::Char(' ') => return Ok(ConfirmResult::Canceled),
                    Key::Tab | Key::Right => focus = ConfirmFocus::Ok,
                    Key::BackTab | Key::Left => {
                        focus = if has_notok {
                            ConfirmFocus::NotOk
                        } else {
                            ConfirmFocus::Ok
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

// -- MESSAGE --

fn run_message(
    terminal: &mut TuiTerminal,
    handle: TtyHandle,
    state: &PinentryState,
) -> miette::Result<()> {
    let title = state.title.as_deref().unwrap_or(env!("CARGO_PKG_NAME"));
    let description = state.description.as_deref().unwrap_or("");
    let ok_label = &state.ok;

    loop {
        terminal
            .draw(|f| {
                let area = centered_rect(60, 8, f.area());
                f.render_widget(Clear, area);
                let block = Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::horizontal(1));
                let inner = block.inner(area);
                f.render_widget(block, area);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(desc_lines(description, inner.width) as u16),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(inner);
                if !description.is_empty() {
                    f.render_widget(
                        Paragraph::new(description).style(Style::default().fg(Color::White)),
                        chunks[0],
                    );
                }
                let btn = Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!(" [ {ok_label} ] "), btn),
                    ])),
                    chunks[2],
                );
            })
            .into_diagnostic()?;

        if let Some(Key::Enter | Key::Esc | Key::Char(' ') | Key::CtrlC) =
            poll_key(handle, Duration::from_millis(100))
        {
            return Ok(());
        }
    }
}

// -- Helpers --

fn desc_lines(text: &str, width: u16) -> usize {
    if text.is_empty() || width == 0 {
        return 0;
    }
    let w = width as usize;
    text.lines()
        .map(|l| {
            let len = l.chars().count();
            if len == 0 { 1 } else { len.div_ceil(w) }
        })
        .sum::<usize>()
        .max(1)
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Max((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Max((area.height.saturating_sub(height)) / 2),
        ])
        .split(area);
    let h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1]);
    h[1]
}

fn btn_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
