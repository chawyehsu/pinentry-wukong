#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::time::{Duration, Instant};

use assuan::ErrorCode;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use miette::IntoDiagnostic;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use crate::state::{PinentryState, SecretBytes};
use crate::ui::PinentryUi;

#[cfg(unix)]
type TtyHandle = std::os::unix::io::RawFd;
#[cfg(windows)]
type TtyHandle = windows_sys::Win32::Foundation::HANDLE;

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

    fn get_pin(&self, state: &PinentryState) -> Result<SecretBytes, ErrorCode> {
        tracing::debug!("TUI: get_pin called");
        let guard = TtyGuard::redirect(state).map_err(|_| ErrorCode::GENERAL)?;
        tracing::debug!("TUI: TtyGuard created, enabling raw mode");
        let mut terminal = create_terminal().map_err(|_| ErrorCode::GENERAL)?;
        tracing::debug!("TUI: terminal created, entering get_pin loop");
        let result = run_getpin(&mut terminal, guard.handle(), state);
        tracing::debug!("TUI: get_pin loop exited with result: {:?}", result.is_ok());
        cleanup_terminal();
        drop(terminal);
        drop(guard);
        result
    }

    fn confirm(&self, state: &PinentryState) -> Result<(), ErrorCode> {
        let guard = TtyGuard::redirect(state).map_err(|_| ErrorCode::GENERAL)?;
        let mut terminal = create_terminal().map_err(|_| ErrorCode::GENERAL)?;
        let result = run_confirm(&mut terminal, guard.handle(), state);
        cleanup_terminal();
        drop(terminal);
        drop(guard);
        result
    }

    fn message(&self, state: &PinentryState) -> Result<(), ErrorCode> {
        let guard = TtyGuard::redirect(state).map_err(|_| ErrorCode::GENERAL)?;
        let mut terminal = create_terminal().map_err(|_| ErrorCode::GENERAL)?;
        let result = run_message(&mut terminal, guard.handle(), state);
        cleanup_terminal();
        drop(terminal);
        drop(guard);
        result
    }
}

// -- Platform dispatch --

#[cfg(unix)]
use unix::{TtyGuard, poll_key};
#[cfg(windows)]
use windows::{TtyGuard, poll_key};

// -- Terminal setup --

fn create_terminal() -> miette::Result<TuiTerminal> {
    enable_raw_mode().into_diagnostic()?;
    let result = (|| {
        execute!(std::io::stdout(), EnterAlternateScreen).into_diagnostic()?;
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend).into_diagnostic()?;
        terminal.clear().into_diagnostic()?;
        Ok(terminal)
    })();
    if result.is_err() {
        let _ = disable_raw_mode();
    }
    result
}

fn cleanup_terminal() {
    if let Err(e) = disable_raw_mode() {
        tracing::warn!("failed to disable raw mode: {e}");
    }
    if let Err(e) = execute!(std::io::stdout(), LeaveAlternateScreen) {
        tracing::warn!("failed to leave alternate screen: {e}");
    }
}

// -- Key input --

/// A parsed key event from terminal input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Left/Right used on Unix
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
) -> Result<SecretBytes, ErrorCode> {
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
            return Err(ErrorCode::CANCELED);
        }

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
            .map_err(|_| ErrorCode::GENERAL)?;

        let poll = timeout
            .map(|t| t.saturating_sub(start.elapsed()))
            .unwrap_or(Duration::from_millis(100));

        if let Some(key) = poll_key(handle, poll) {
            match handle_getpin_key(key, &mut focus, &mut input) {
                GetPinAction::Continue => {}
                GetPinAction::Submit => {
                    return Ok(SecretBytes::from(input.into_bytes()));
                }
                GetPinAction::Cancel => return Err(ErrorCode::CANCELED),
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
) -> Result<(), ErrorCode> {
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
            return Err(ErrorCode::CANCELED);
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
            .map_err(|_| ErrorCode::GENERAL)?;

        let poll = timeout
            .map(|t| t.saturating_sub(start.elapsed()))
            .unwrap_or(Duration::from_millis(100));
        if let Some(key) = poll_key(handle, poll) {
            if key == Key::CtrlC || key == Key::Esc {
                return Err(ErrorCode::CANCELED);
            }
            match focus {
                ConfirmFocus::Ok => match key {
                    Key::Enter | Key::Char(' ') => return Ok(()),
                    Key::Tab | Key::Right => focus = focus.next(has_notok),
                    Key::BackTab | Key::Left => focus = ConfirmFocus::Cancel,
                    _ => {}
                },
                ConfirmFocus::NotOk => match key {
                    Key::Enter | Key::Char(' ') => return Err(ErrorCode::NOT_CONFIRMED),
                    Key::Tab | Key::Right => focus = focus.next(has_notok),
                    Key::BackTab | Key::Left => focus = ConfirmFocus::Ok,
                    _ => {}
                },
                ConfirmFocus::Cancel => match key {
                    Key::Enter | Key::Char(' ') => return Err(ErrorCode::CANCELED),
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
) -> Result<(), ErrorCode> {
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
            .map_err(|_| ErrorCode::GENERAL)?;

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
