pub mod detect;
pub mod tty;
pub mod tui;

use crate::state::{ConfirmResult, GetPinResult, PinentryState};

/// Trait for pinentry UI
///
/// Each backend (TUI, TTY, etc.) implements this trait to provide
/// passphrase entry, confirmation dialogs, and message display.
pub trait PinentryUi {
    /// Return the UI flavor identifier
    fn flavor(&self) -> &str;

    /// Prompt the user for a passphrase
    fn get_pin(&self, state: &PinentryState) -> miette::Result<GetPinResult>;

    /// Show a confirmation dialog (OK / Cancel / Not-OK)
    fn confirm(&self, state: &PinentryState) -> miette::Result<ConfirmResult>;

    /// Show a one-button message dialog
    fn message(&self, state: &PinentryState) -> miette::Result<()>;
}
