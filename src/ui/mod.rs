pub mod detect;
pub mod tty;
pub mod tui;
#[cfg(windows)]
pub mod windows;

use assuan::ErrorCode;

use crate::state::{PinentryState, SecretBytes};

/// Trait for pinentry UI
///
/// Each backend (TUI, TTY, etc.) implements this trait to provide
/// passphrase entry, confirmation dialogs, and message display.
pub trait PinentryUi {
    /// Return the UI flavor identifier
    fn flavor(&self) -> &str;

    /// Prompt the user for a passphrase
    fn get_pin(&self, state: &PinentryState) -> Result<SecretBytes, ErrorCode>;

    /// Show a confirmation dialog (OK / Cancel / Not-OK)
    fn confirm(&self, state: &PinentryState) -> Result<(), ErrorCode>;

    /// Show a one-button message dialog
    fn message(&self, state: &PinentryState) -> Result<(), ErrorCode>;
}
