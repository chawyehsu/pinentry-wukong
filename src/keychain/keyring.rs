use keyring::Entry;

use crate::keychain::Keychain;
use crate::state::SecretBytes;

const SERVICE_NAME: &str = "GnuPG";

/// Extract the keygrip from the SETKEYINFO value.
///
/// gpg-agent sends SETKEYINFO as `n/<keygrip>` where `n/` is a prefix.
/// We need just the keygrip hex string for the keychain account name.
fn extract_keygrip(keyinfo: &str) -> &str {
    keyinfo.strip_prefix("n/").unwrap_or(keyinfo)
}

/// Cross-platform keychain implementation using the `keyring` crate.
pub struct KeyringKeychain;

impl KeyringKeychain {
    pub fn new() -> Self {
        Self
    }
}

impl Keychain for KeyringKeychain {
    fn lookup(&self, keyinfo: &str) -> Option<SecretBytes> {
        let keygrip = extract_keygrip(keyinfo);
        tracing::debug!("keychain lookup: service={SERVICE_NAME}, account={keygrip}");
        let entry = match Entry::new(SERVICE_NAME, keygrip) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("keychain: failed to create entry: {e}");
                return None;
            }
        };
        match entry.get_password() {
            Ok(password) => Some(SecretBytes::from(password.into_bytes())),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                tracing::debug!("keychain lookup failed for key {keygrip}: {e}");
                None
            }
        }
    }

    fn save(&self, keyinfo: &str, passphrase: &[u8]) -> miette::Result<()> {
        let keygrip = extract_keygrip(keyinfo);
        tracing::debug!("keychain save: service={SERVICE_NAME}, account={keygrip}");

        let entry = Entry::new(SERVICE_NAME, keygrip)
            .map_err(|e| miette::miette!("failed to create keyring entry: {e}"))?;

        let password = std::str::from_utf8(passphrase)
            .map_err(|_| miette::miette!("passphrase is not valid UTF-8"))?;

        entry
            .set_password(password)
            .map_err(|e| miette::miette!("failed to save passphrase to keyring: {e}"))
    }

    fn clear(&self, keyinfo: &str) -> miette::Result<bool> {
        let keygrip = extract_keygrip(keyinfo);
        tracing::debug!("keychain clear: service={SERVICE_NAME}, account={keygrip}");

        let entry = Entry::new(SERVICE_NAME, keygrip)
            .map_err(|e| miette::miette!("failed to create keyring entry: {e}"))?;

        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(miette::miette!("failed to clear keyring entry: {e}")),
        }
    }
}
