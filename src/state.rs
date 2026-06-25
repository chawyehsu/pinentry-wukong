use zeroize::Zeroize;

/// Result of a GETPIN operation
#[derive(Debug)]
pub enum GetPinResult {
    /// User entered a passphrase
    Pin(SecretBytes),
    /// User canceled the operation
    Canceled,
    /// Window was closed
    Closed,
}

/// Result of a CONFIRM operation
#[derive(Debug)]
pub enum ConfirmResult {
    /// User accepted (OK button)
    Accepted,
    /// User canceled (Cancel button)
    Canceled,
    /// User pressed the "not ok" button
    NotOk,
    /// Window was closed
    Closed,
}

/// A byte buffer that is zeroed on drop
#[derive(Clone, Debug, Zeroize)]
#[zeroize(drop)]
pub struct SecretBytes(pub Vec<u8>);

#[allow(dead_code)]
impl SecretBytes {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl Default for SecretBytes {
    fn default() -> Self {
        Self::new()
    }
}

/// The core pinentry state
///
/// This holds all configuration received via Assuan commands before
/// a GETPIN/CONFIRM/MESSAGE is processed.
#[derive(Debug)]
pub struct PinentryState {
    // -- UI text fields --
    /// Window title (SETTITLE)
    pub title: Option<String>,
    /// Description text (SETDESC)
    pub description: Option<String>,
    /// Error message (SETERROR)
    pub error: Option<String>,
    /// Input prompt label (SETPROMPT)
    ///
    /// Default: `Passphrase:`
    pub prompt: String,
    /// OK button label (SETOK)
    ///
    /// Default: `OK`
    pub ok: String,
    /// "Not OK" button label (SETNOTOK)
    pub notok: Option<String>,
    /// Cancel button label (SETCANCEL)
    ///
    /// Default: `Cancel`
    pub cancel: String,

    // -- Keychain caching --
    /// Key identifier for keychain caching (SETKEYINFO)
    pub keyinfo: Option<String>,
    /// Whether external password cache is allowed (OPTION allow-external-password-cache)
    pub allow_external_password_cache: bool,
    /// Whether we've already tried the password cache this session
    pub tried_password_cache: bool,
    /// Whether the user consented to caching the password
    pub may_cache_password: bool,
    /// Whether the passphrase was read from cache
    pub pin_from_cache: bool,

    // -- Options (set via OPTION command) --
    /// X display name
    pub display: Option<String>,
    /// TTY name
    pub ttyname: Option<String>,
    /// TTY type
    pub ttytype: Option<String>,
    /// LC_CTYPE value
    pub lc_ctype: Option<String>,
    /// LC_MESSAGES value
    pub lc_messages: Option<String>,
    /// Timeout in seconds (default: 60)
    pub timeout: u32,
    /// Whether to grab keyboard focus
    pub grab: bool,

    // -- Repeat passphrase (deferred) --
    pub repeat_passphrase: Option<String>,
    pub repeat_error_string: Option<String>,
    pub repeat_ok_string: Option<String>,

    // -- Quality bar (deferred) --
    pub quality_bar: Option<String>,
    pub quality_bar_tt: Option<String>,

    // -- Confirm mode flags --
    /// Whether this is a single-button (message) dialog
    pub one_button: bool,
}

impl Default for PinentryState {
    fn default() -> Self {
        Self::new()
    }
}

impl PinentryState {
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            error: None,
            prompt: "Passphrase:".to_string(),
            ok: "OK".to_string(),
            notok: None,
            cancel: "Cancel".to_string(),
            keyinfo: None,
            allow_external_password_cache: false,
            tried_password_cache: false,
            may_cache_password: false,
            pin_from_cache: false,
            display: None,
            ttyname: None,
            ttytype: None,
            lc_ctype: None,
            lc_messages: None,
            timeout: 60, // default timeout
            grab: true,  // default to grabbing keyboard focus
            repeat_passphrase: None,
            repeat_error_string: None,
            repeat_ok_string: None,
            quality_bar: None,
            quality_bar_tt: None,
            one_button: false,
        }
    }

    /// Reset per-request fields (title, description, error, prompt, buttons).
    /// Preserves session-level options (grab, display, tty, timeout, keyinfo, etc.).
    pub fn reset(&mut self) {
        self.title = None;
        self.description = None;
        self.error = None;
        self.prompt = "Passphrase:".to_string();
        self.ok = "OK".to_string();
        self.notok = None;
        self.cancel = "Cancel".to_string();
        self.repeat_passphrase = None;
        self.repeat_error_string = None;
        self.repeat_ok_string = None;
        self.quality_bar = None;
        self.quality_bar_tt = None;
        self.pin_from_cache = false;
        self.may_cache_password = false;
    }
}
