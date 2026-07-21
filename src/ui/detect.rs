use std::fmt;

/// Which UI backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// ratatui-based terminal UI (requires a TTY)
    Tui,
    /// Simple line-based TTY fallback
    Tty,
    /// Auto-detect based on terminal capabilities
    Auto,
}

impl fmt::Display for UiMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiMode::Tui => write!(f, "tui"),
            UiMode::Tty => write!(f, "tty"),
            UiMode::Auto => write!(f, "auto"),
        }
    }
}

impl std::str::FromStr for UiMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tui" => Ok(UiMode::Tui),
            "tty" => Ok(UiMode::Tty),
            "auto" => Ok(UiMode::Auto),
            _ => Err(format!("unknown UI mode: {s} (valid: auto, tty, tui)")),
        }
    }
}

impl UiMode {
    /// Resolve to a concrete UI mode (Tui or Tty).
    ///
    /// `Auto` → detect from terminal. `PreferTty` → try tty first. `PreferGui` → try gui first.
    /// Since GUI is not yet supported, both `PreferTty` and `PreferGui` resolve to the
    /// best available terminal mode.
    pub fn resolve(self) -> Self {
        match self {
            UiMode::Tui | UiMode::Tty => self,
            UiMode::Auto => detect_ui_mode(),
        }
    }

    /// Create the UI backend for this mode.
    ///
    /// Panics if called on an unresolved mode (Auto, PreferTty, PreferGui).
    pub fn create_ui(&self) -> Box<dyn crate::ui::PinentryUi> {
        match self {
            UiMode::Tui => Box::new(super::tui::TuiUi::new()),
            UiMode::Tty => Box::new(super::tty::TtyUi::new()),
            _ => panic!("UiMode::create_ui called on unresolved mode: {self}"),
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
