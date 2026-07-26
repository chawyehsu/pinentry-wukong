use std::time::Duration;

use crossterm::terminal::disable_raw_mode;
use miette::IntoDiagnostic;

use super::Key;
use crate::state::PinentryState;

pub(super) struct TtyGuard;

impl TtyGuard {
    pub(super) fn redirect(_state: &PinentryState) -> miette::Result<Self> {
        // On Windows, crossterm uses ReadConsoleInputW/WriteConsoleOutputW
        // which don't conflict with stdin/stdout fd redirections.
        // No fd manipulation needed — stdin/stdout work directly.
        tracing::debug!("TUI: Windows — no fd redirection needed");
        Ok(Self)
    }

    pub(super) fn handle(&self) {}
}

impl Drop for TtyGuard {
    fn drop(&mut self) {
        tracing::debug!("TUI: Windows — no fd restoration needed");
    }
}

pub(super) fn cleanup_terminal(_handle: ()) -> miette::Result<()> {
    disable_raw_mode().into_diagnostic()?;
    // On Windows, crossterm handles terminal cleanup via the Console API.
    // The escape sequences are processed by the Windows terminal directly.
    Ok(())
}

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

pub(super) fn poll_key(_handle: (), timeout: Duration) -> Option<Key> {
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
