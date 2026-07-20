use std::io;

use super::PinentryServer;

/// Run the pinentry server on stdin/stdout.
///
/// On Windows, crossterm uses `ReadConsoleInputW` via `GetStdHandle(STD_INPUT_HANDLE)`
/// internally, which doesn't conflict with `stdin()` the way Unix's shared-fd-0 model
/// does. So we can use stdin/stdout directly without the fd-dup dance.
pub fn start(
    ui: &dyn crate::ui::PinentryUi,
    grab: bool,
    timeout: u32,
    keychain: Option<Box<dyn crate::keychain::Keychain>>,
) -> miette::Result<()> {
    let reader = io::stdin();
    let writer = io::stdout();

    let mut server = PinentryServer::new(reader, writer, grab, timeout, keychain);
    server.run(ui).map_err(|e| miette::miette!("{e}"))
}
