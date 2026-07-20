use std::io::{Read, Write};

use assuan::{self, Error, ErrorCode, Request, Response};

use crate::keychain::Keychain;
use crate::server::command::Command;
use crate::state::{ConfirmResult, GetPinResult, PinentryState};
use crate::ui::PinentryUi;

/// The pinentry assuan server, which handles the Assuan protocol and
/// delegates UI interactions to a PinentryUi implementation.
pub struct PinentryServer<R: Read, W: Write> {
    /// The underlying Assuan server that handles assuan protocol
    inner: assuan::Server<R, W>,

    /// The current pinentry state managed by the server
    state: PinentryState,

    /// Optional keychain backend for pin caching
    keychain: Option<Box<dyn Keychain>>,
}

impl<R: Read, W: Write> PinentryServer<R, W> {
    /// Create a new PinentryServer
    pub fn new(
        reader: R,
        writer: W,
        grab: bool,
        timeout: u32,
        keychain: Option<Box<dyn Keychain>>,
    ) -> Self {
        Self {
            inner: assuan::Server::new(reader, writer),
            state: PinentryState {
                timeout,
                grab,
                ..PinentryState::default()
            },
            keychain,
        }
    }

    /// Start the server loop
    ///
    /// This function runs the main server loop, reading Assuan requests and
    /// dispatching them to the appropriate handlers. The loop continues until
    /// a BYE command is received or an error occurs.
    pub fn run(&mut self, ui: &dyn PinentryUi) -> Result<(), Error> {
        self.send(Response::ok("Pleased to meet you!"))?;

        loop {
            let req = match self.inner.recv() {
                Ok(Some(req)) => req,
                Ok(None) => break, // BYE or EOF
                Err(e) => {
                    tracing::error!("read error: {e}");
                    break;
                }
            };

            // The assuan::Server handles BYE, NOP, and Comment transparently.
            // We only see application commands, Reset, and Option here.
            match req {
                Request::Reset => {
                    self.state.reset();
                    // OK already sent by assuan::Server.
                    continue;
                }
                Request::End => {
                    let _ = self.send_err(ErrorCode::GENERAL, Some("unexpected END"));
                    continue;
                }
                Request::Option { key, value } => {
                    if let Err(e) = self.handle_option(&key, &value) {
                        let _ = self.send(e.into());
                    } else {
                        self.send(Response::OK)?;
                    }
                    continue;
                }
                Request::Command { .. } => match Command::try_from(req) {
                    Ok(cmd) => {
                        if let Err(e) = self.handle_command(cmd, ui) {
                            tracing::error!("command error: {e}");
                            let _ = self.send(e.into());
                        }
                    }
                    Err(e) => {
                        tracing::error!("command parse error: {e}");
                        let _ = self.send(e.into());
                    }
                },
                _ => {
                    // Other protocol commands (should not reach here since
                    // assuan::Server handles Bye/Nop/Comment).
                    continue;
                }
            }
        }

        Ok(())
    }

    fn handle_command(&mut self, cmd: Command, ui: &dyn PinentryUi) -> Result<(), Error> {
        // Commands that send their own OK (with D lines or custom sequencing).
        let sends_own_ok = matches!(
            cmd,
            Command::GetPin | Command::Confirm { .. } | Command::Message | Command::GetInfo(_)
        );

        match cmd {
            Command::SetDesc(s) => self.state.description = Some(s),
            Command::SetPrompt(s) => self.state.prompt = s,
            Command::SetError(s) => self.state.error = Some(s),
            Command::SetTitle(s) => self.state.title = Some(s),
            Command::SetOk(s) => self.state.ok = s,
            Command::SetNotOk(s) => self.state.notok = Some(s),
            Command::SetCancel(s) => self.state.cancel = s,
            Command::SetKeyInfo(s) => {
                self.state.keyinfo = if s.is_empty() { None } else { Some(s) };
            }
            Command::ClearPassphrase(keygrip) => {
                if let Some(ref keychain) = self.keychain {
                    match keychain.clear(&keygrip) {
                        Ok(true) => tracing::info!("cleared passphrase for key {keygrip}"),
                        Ok(false) => tracing::debug!("no passphrase found for key {keygrip}"),
                        Err(e) => {
                            tracing::warn!("failed to clear passphrase for key {keygrip}: {e}")
                        }
                    }
                }
            }

            Command::SetQualityBar(s) => self.state.quality_bar = Some(s),
            Command::SetQualityBarTt(s) => self.state.quality_bar_tt = Some(s),
            Command::SetRepeat(s) => self.state.repeat_passphrase = Some(s),
            Command::SetRepeatError(s) => self.state.repeat_error_string = Some(s),
            Command::SetRepeatOk(s) => self.state.repeat_ok_string = Some(s),
            Command::SetGenPin(_) | Command::SetGenPinTt(_) => {}

            Command::SetTimeout(secs) => self.state.timeout = secs,

            Command::GetPin => self.handle_getpin(ui)?,
            Command::Confirm { one_button } => self.handle_confirm(ui, one_button)?,
            Command::Message => self.handle_message(ui)?,
            Command::GetInfo(what) => self.handle_getinfo(&what, ui)?,
        }

        // Commands that don't send their own OK get one here.
        if !sends_own_ok {
            self.send(Response::OK)?;
        }

        Ok(())
    }

    fn handle_option(&mut self, key: &str, value: &str) -> Result<(), Error> {
        match key {
            "no-grab" => self.state.grab = false,
            "grab" => self.state.grab = true,
            "display" => self.state.display = Some(value.to_string()),
            "ttyname" => self.state.ttyname = Some(value.to_string()),
            "ttytype" => self.state.ttytype = Some(value.to_string()),
            "lc-ctype" => self.state.lc_ctype = Some(value.to_string()),
            "lc-messages" => self.state.lc_messages = Some(value.to_string()),
            "allow-external-password-cache" => {
                self.state.allow_external_password_cache = true;
                self.state.tried_password_cache = false;
                // If we have a keychain backend, automatically allow saving
                if self.keychain.is_some() {
                    self.state.may_cache_password = true;
                }
            }
            "owner" | "parent-wid" | "touch-file" => {}
            "default-ok" | "default-cancel" | "default-prompt" | "default-pwmngr"
            | "default-cf-visi" | "default-tt-visi" | "default-tt-hide" | "default-capshint"
            | "default-yes" | "default-no" => {}
            "invisible-char"
            | "formatted-passphrase"
            | "formatted-passphrase-hint"
            | "constraints-enforce"
            | "constraints-hint-short"
            | "constraints-hint-long"
            | "constraints-error-title"
            | "allow-emacs-prompt"
            | "debug-wait" => {}
            _ => {
                return Err(Error::new(
                    ErrorCode::ASS_UNKNOWN_CMD,
                    format!("unknown option: {key}"),
                ));
            }
        }
        Ok(())
    }

    fn handle_getpin(&mut self, ui: &dyn PinentryUi) -> Result<(), Error> {
        tracing::debug!("GETPIN: handling request");
        // Try keychain lookup first
        if self.state.allow_external_password_cache
            && self.state.keyinfo.is_some()
            && !self.state.tried_password_cache
            && self.state.error.is_none()
        {
            self.state.tried_password_cache = true;
            let keygrip = self.state.keyinfo.as_ref().unwrap();

            if let Some(ref keychain) = self.keychain
                && let Some(cached) = keychain.lookup(keygrip)
                && !cached.is_empty()
            {
                tracing::info!(
                    "password found in cache for key {keygrip} ({} bytes)",
                    cached.len()
                );
                self.state.pin_from_cache = true;
                self.send(Response::status("PASSWORD_FROM_CACHE", ""))?;
                self.send(Response::data(cached.as_bytes().to_vec()))?;
                self.send(Response::OK)?;
                return Ok(());
            }
        }

        tracing::debug!("GETPIN: prompting user via UI");
        let result = ui
            .get_pin(&self.state)
            .map_err(|e| Error::new(ErrorCode::GENERAL, e.to_string()))?;

        self.state.error = None;

        match result {
            GetPinResult::Pin(secret) => {
                if !secret.is_empty() {
                    tracing::debug!(
                        "GETPIN: passphrase entered ({} bytes), cache_allowed={}, keyinfo={:?}, may_cache={}",
                        secret.len(),
                        self.state.allow_external_password_cache,
                        self.state.keyinfo,
                        self.state.may_cache_password
                    );

                    if self.state.allow_external_password_cache
                        && let Some(ref keygrip) = self.state.keyinfo
                        && self.state.may_cache_password
                    {
                        tracing::debug!("saving passphrase to keychain for key {keygrip}");
                        if let Some(ref keychain) = self.keychain {
                            match keychain.save(keygrip, secret.as_bytes()) {
                                Ok(()) => {
                                    tracing::info!("passphrase saved to keychain for key {keygrip}")
                                }
                                Err(e) => {
                                    tracing::warn!("failed to save password to keychain: {e}")
                                }
                            }
                        } else {
                            tracing::warn!("keychain save requested but no keychain backend");
                        }
                    } else {
                        tracing::debug!(
                            "keychain save skipped: cache_allowed={}, keyinfo={:?}, may_cache={}",
                            self.state.allow_external_password_cache,
                            self.state.keyinfo,
                            self.state.may_cache_password
                        );
                    }

                    tracing::debug!("GETPIN: sending D line ({} bytes)", secret.len());
                    self.send(Response::data(secret.as_bytes().to_vec()))?;
                }
                tracing::debug!("GETPIN: sending OK");
                self.send(Response::OK)?;
            }
            GetPinResult::Canceled => {
                return Err(ErrorCode::CANCELED.into());
            }
            GetPinResult::Closed => {
                self.send(Response::status("BUTTON_INFO", "close"))?;
                return Err(ErrorCode::CANCELED.into());
            }
        }

        self.state.repeat_passphrase = None;
        Ok(())
    }

    fn handle_confirm(&mut self, ui: &dyn PinentryUi, one_button: bool) -> Result<(), Error> {
        if one_button {
            self.state.one_button = true;
        }

        let result = ui
            .confirm(&self.state)
            .map_err(|e| Error::new(ErrorCode::GENERAL, e.to_string()))?;

        match result {
            ConfirmResult::Accepted => {
                self.send(Response::OK)?;
            }
            ConfirmResult::Canceled => return Err(ErrorCode::CANCELED.into()),
            ConfirmResult::NotOk => return Err(ErrorCode::NOT_CONFIRMED.into()),
            ConfirmResult::Closed => {
                self.send(Response::status("BUTTON_INFO", "close"))?;
                return Err(ErrorCode::CANCELED.into());
            }
        }

        Ok(())
    }

    fn handle_message(&mut self, ui: &dyn PinentryUi) -> Result<(), Error> {
        ui.message(&self.state)
            .map_err(|e| Error::new(ErrorCode::GENERAL, e.to_string()))?;
        self.send(Response::OK)?;
        Ok(())
    }

    fn handle_getinfo(&mut self, what: &str, ui: &dyn PinentryUi) -> Result<(), Error> {
        let data = match what {
            "version" => env!("CARGO_PKG_VERSION").to_string(),
            "pid" => std::process::id().to_string(),
            "flavor" => ui.flavor().to_string(),
            "ttyinfo" => {
                let ttyname = self.state.ttyname.as_deref().unwrap_or("-");
                let ttytype = self.state.ttytype.as_deref().unwrap_or("-");
                let display = self.state.display.as_deref().unwrap_or("-");
                format!("{ttyname} {ttytype} {display} - -")
            }
            _ => return Err(ErrorCode::INV_PARAMETER.into()),
        };

        self.send(Response::data(data.into_bytes()))?;
        self.send(Response::OK)?;
        Ok(())
    }

    // -- I/O helpers --

    fn send(&mut self, resp: Response) -> Result<(), Error> {
        self.inner.send(resp)?;
        Ok(())
    }

    fn send_err(&mut self, code: ErrorCode, msg: Option<&str>) -> Result<(), Error> {
        self.inner
            .send(Response::err(code, msg.map(|s| s.to_string())))?;
        Ok(())
    }
}
