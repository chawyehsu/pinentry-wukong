pub mod keyring;
pub mod noop;

use crate::state::SecretBytes;

/// Trait for OS-specific credential store backends.
///
/// Each platform (macOS Keychain, Linux Secret Service, Windows Credential Manager)
/// implements this trait to provide passphrase caching.
pub trait Keychain: Send + Sync {
    /// Look up a cached passphrase by keygrip.
    fn lookup(&self, keygrip: &str) -> Option<SecretBytes>;

    /// Save a passphrase for the given keygrip.
    fn save(&self, keygrip: &str, passphrase: &[u8]) -> miette::Result<()>;

    /// Clear a cached passphrase by keygrip. Returns true if found and removed.
    fn clear(&self, keygrip: &str) -> miette::Result<bool>;
}
