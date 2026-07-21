#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use crate::state::{ConfirmResult, GetPinResult, PinentryState};
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

    fn get_pin(&self, state: &PinentryState) -> miette::Result<GetPinResult> {
        #[cfg(unix)]
        {
            unix::get_pin(state)
        }
        #[cfg(windows)]
        {
            windows::get_pin(state)
        }
    }

    fn confirm(&self, state: &PinentryState) -> miette::Result<ConfirmResult> {
        #[cfg(unix)]
        {
            unix::confirm(state)
        }
        #[cfg(windows)]
        {
            windows::confirm(state)
        }
    }

    fn message(&self, state: &PinentryState) -> miette::Result<()> {
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
