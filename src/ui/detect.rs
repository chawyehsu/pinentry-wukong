use std::fmt;

/// Which UI backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// ratatui-based terminal UI (requires a TTY)
    Tui,
    /// Simple line-based TTY fallback
    Tty,
}

impl fmt::Display for UiMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiMode::Tui => write!(f, "tui"),
            UiMode::Tty => write!(f, "tty"),
        }
    }
}

impl std::str::FromStr for UiMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tui" => Ok(UiMode::Tui),
            "tty" => Ok(UiMode::Tty),
            _ => Err(format!("unknown UI mode: {s} (valid: tui, tty)")),
        }
    }
}

impl UiMode {
    /// Create the UI backend for this mode.
    pub fn create_ui(&self) -> Box<dyn crate::ui::PinentryUi> {
        match self {
            UiMode::Tui => Box::new(super::tui::TuiUi::new()),
            UiMode::Tty => Box::new(super::tty::TtyUi::new()),
        }
    }
}

/// Detect the appropriate UI mode.
///
/// Default to TUI. The actual terminal device is determined at runtime using
/// `OPTION ttyname` from gpg-agent (e.g. `/dev/ttys002`) or `/dev/tty` as
/// fallback. The UI backends handle opening the device directly.
///
/// Only fall back to TTY if `TERM` is `dumb` or unset.
pub fn detect_ui_mode() -> UiMode {
    match std::env::var("TERM") {
        Ok(term) if term != "dumb" && !term.is_empty() => UiMode::Tui,
        _ => UiMode::Tty,
    }
}
