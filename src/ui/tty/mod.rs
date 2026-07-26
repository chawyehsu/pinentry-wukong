#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use assuan::ErrorCode;

use crate::state::{PinentryState, SecretBytes};
use crate::ui::PinentryUi;

pub struct TtyUi;

impl TtyUi {
    pub fn new() -> Self {
        Self
    }
}

impl PinentryUi for TtyUi {
    fn flavor(&self) -> &str {
        "wukong:tty"
    }

    fn get_pin(&self, state: &PinentryState) -> Result<SecretBytes, ErrorCode> {
        #[cfg(unix)]
        {
            unix::get_pin(state)
        }
        #[cfg(windows)]
        {
            windows::get_pin(state)
        }
    }

    fn confirm(&self, state: &PinentryState) -> Result<(), ErrorCode> {
        #[cfg(unix)]
        {
            unix::confirm(state)
        }
        #[cfg(windows)]
        {
            windows::confirm(state)
        }
    }

    fn message(&self, state: &PinentryState) -> Result<(), ErrorCode> {
        #[cfg(unix)]
        {
            unix::message(state)
        }
        #[cfg(windows)]
        {
            windows::message(state)
        }
    }
}
