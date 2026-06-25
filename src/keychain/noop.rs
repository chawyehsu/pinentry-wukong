#![allow(dead_code)]
use crate::keychain::Keychain;
use crate::state::SecretBytes;

/// No-op keychain implementation
///
/// This implementation does not store or retrieve any passphrases.
/// It is useful for testing or when keychain functionality is not desired.
/// Also it is used when the keychain feature is disabled.
pub struct NoopKeychain;

impl NoopKeychain {
    pub fn new() -> Self {
        Self
    }
}

impl Keychain for NoopKeychain {
    fn lookup(&self, _keygrip: &str) -> Option<SecretBytes> {
        None
    }

    fn save(&self, _keygrip: &str, _passphrase: &[u8]) -> miette::Result<()> {
        Ok(())
    }

    fn clear(&self, _keygrip: &str) -> miette::Result<bool> {
        Ok(false)
    }
}
