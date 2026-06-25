use assuan::{Error, ErrorCode, Request};

/// Parsed Assuan commands for pinentry server
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // -- UI text --
    /// Set the description text (SETDESC)
    SetDesc(String),
    /// Set the prompt text (SETPROMPT)
    SetPrompt(String),
    /// Set the error text (SETERROR)
    SetError(String),
    /// Set the window title (SETTITLE)
    SetTitle(String),
    /// Set the OK button label (SETOK)
    SetOk(String),
    /// Set the "not ok" button label (SETNOTOK)
    SetNotOk(String),
    /// Set the Cancel button label (SETCANCEL)
    SetCancel(String),

    // -- Keychain caching --
    /// Set the key identifier for keychain caching (SETKEYINFO)
    SetKeyInfo(String),
    /// Clear the cached passphrase for a given key (CLEARPASSPHRASE)
    ClearPassphrase(String),

    // -- Quality bar (deferred but parsed) --
    SetQualityBar(String),
    SetQualityBarTt(String),

    // -- Repeat passphrase (deferred but parsed) --
    SetRepeat(String),
    SetRepeatError(String),
    SetRepeatOk(String),

    // -- Generate PIN (deferred but parsed) --
    SetGenPin(String),
    SetGenPinTt(String),

    // -- Timeout --
    SetTimeout(u32),

    // -- Actions --
    /// Get the PIN (GETPIN)
    GetPin,
    /// Confirm the action (CONFIRM)
    Confirm {
        one_button: bool,
    },
    /// Display a message (MESSAGE)
    Message,
    /// Get information (GETINFO)
    GetInfo(String),
}

impl TryFrom<Request> for Command {
    type Error = Error;

    fn try_from(req: Request) -> Result<Self, Self::Error> {
        let Request::Command { name, args } = req else {
            unreachable!("AssuanCommand::try_from called on non-Command request");
        };

        let args = args.unwrap_or_default();
        match name.as_str() {
            // UI text setters
            "SETDESC" => Ok(Command::SetDesc(args)),
            "SETPROMPT" => Ok(Command::SetPrompt(args)),
            "SETERROR" => Ok(Command::SetError(args)),
            "SETTITLE" => Ok(Command::SetTitle(args)),
            "SETOK" => Ok(Command::SetOk(args)),
            "SETNOTOK" => Ok(Command::SetNotOk(args)),
            "SETCANCEL" => Ok(Command::SetCancel(args)),

            // Keychain / caching
            "SETKEYINFO" => {
                if args.is_empty() || args == "--clear" {
                    Ok(Command::SetKeyInfo(String::new()))
                } else {
                    Ok(Command::SetKeyInfo(args))
                }
            }
            "CLEARPASSPHRASE" => {
                if args.is_empty() {
                    Err(Error::new(ErrorCode::INV_PARAMETER, "empty key"))
                } else {
                    Ok(Command::ClearPassphrase(args))
                }
            }

            // Quality bar
            "SETQUALITYBAR" => Ok(Command::SetQualityBar(args)),
            "SETQUALITYBAR_TT" => Ok(Command::SetQualityBarTt(args)),

            // Repeat passphrase
            "SETREPEAT" => Ok(Command::SetRepeat(args)),
            "SETREPEATERROR" => Ok(Command::SetRepeatError(args)),
            "SETREPEATOK" => Ok(Command::SetRepeatOk(args)),

            // Generate PIN
            "SETGENPIN" => Ok(Command::SetGenPin(args)),
            "SETGENPIN_TT" => Ok(Command::SetGenPinTt(args)),

            // Timeout
            "SETTIMEOUT" => {
                if args.is_empty() {
                    Ok(Command::SetTimeout(0))
                } else {
                    let secs = args
                        .parse::<u32>()
                        .map_err(|_| Error::new(ErrorCode::INV_PARAMETER, "invalid timeout"))?;
                    Ok(Command::SetTimeout(secs))
                }
            }

            // Actions
            "GETPIN" => Ok(Command::GetPin),
            "CONFIRM" => {
                let one_button = args.contains("--one-button");
                Ok(Command::Confirm { one_button })
            }
            "MESSAGE" => Ok(Command::Message),
            "GETINFO" => Ok(Command::GetInfo(args)),

            _ => Err(Error::new(
                ErrorCode::ASS_UNKNOWN_CMD,
                format!("unknown command: {name}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_line(line: &[u8]) -> Command {
        let mut buf = line.to_vec();
        let req = Request::parse(&mut buf).unwrap();
        Command::try_from(req).unwrap()
    }

    #[test]
    fn test_parse_setdesc() {
        let cmd = parse_line(b"SETDESC Enter passphrase to unlock");
        assert_eq!(cmd, Command::SetDesc("Enter passphrase to unlock".into()));
    }

    #[test]
    fn test_parse_setprompt() {
        let cmd = parse_line(b"SETPROMPT Passphrase:");
        assert_eq!(cmd, Command::SetPrompt("Passphrase:".into()));
    }

    #[test]
    fn test_parse_seterror() {
        let cmd = parse_line(b"SETERROR Bad passphrase");
        assert_eq!(cmd, Command::SetError("Bad passphrase".into()));
    }

    #[test]
    fn test_parse_settitle() {
        let cmd = parse_line(b"SETTITLE Unlock Key");
        assert_eq!(cmd, Command::SetTitle("Unlock Key".into()));
    }

    #[test]
    fn test_parse_getpin() {
        let cmd = parse_line(b"GETPIN");
        assert_eq!(cmd, Command::GetPin);
    }

    #[test]
    fn test_parse_confirm() {
        let cmd = parse_line(b"CONFIRM");
        assert_eq!(cmd, Command::Confirm { one_button: false });
    }

    #[test]
    fn test_parse_confirm_one_button() {
        let cmd = parse_line(b"CONFIRM --one-button");
        assert_eq!(cmd, Command::Confirm { one_button: true });
    }

    #[test]
    fn test_parse_message() {
        let cmd = parse_line(b"MESSAGE");
        assert_eq!(cmd, Command::Message);
    }

    #[test]
    fn test_parse_setkeyinfo() {
        let cmd = parse_line(b"SETKEYINFO ABC123");
        assert_eq!(cmd, Command::SetKeyInfo("ABC123".into()));
    }

    #[test]
    fn test_parse_setkeyinfo_clear() {
        let cmd = parse_line(b"SETKEYINFO --clear");
        assert_eq!(cmd, Command::SetKeyInfo(String::new()));
    }

    #[test]
    fn test_parse_settimeout() {
        let cmd = parse_line(b"SETTIMEOUT 30");
        assert_eq!(cmd, Command::SetTimeout(30));
    }

    #[test]
    fn test_parse_settimeout_empty() {
        let cmd = parse_line(b"SETTIMEOUT");
        assert_eq!(cmd, Command::SetTimeout(0));
    }

    #[test]
    fn test_parse_unknown() {
        let mut buf = b"FOOBAR".to_vec();
        let req = Request::parse(&mut buf).unwrap();
        let result = Command::try_from(req);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_case_insensitive() {
        // Command names from Request are already uppercased.
        let cmd = parse_line(b"getpin");
        assert_eq!(cmd, Command::GetPin);
    }
}
