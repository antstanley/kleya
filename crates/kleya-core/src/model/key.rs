//! Key newtypes — KeyName/PublicKey/Fingerprint with private fields.

#![allow(missing_docs)]

use crate::error::{Error, Result};
use crate::limits::KEY_NAME_BYTES_MAX;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static KEY_NAME_RE: Lazy<Regex> = Lazy::new(|| {
    // Leading char excludes '.' to prevent hidden-file or traversal segments.
    Regex::new(r"^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,127}$").expect("static regex compiles")
});

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static FINGERPRINT_RE: Lazy<Regex> = Lazy::new(|| {
    // EC2 MD5-of-DER-SPKI: 16 colon-separated lowercase hex bytes (47 chars).
    Regex::new(r"^[0-9a-f]{2}(:[0-9a-f]{2}){15}$").expect("static regex compiles")
});

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyName(String);

impl KeyName {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(Error::ConfigInvalid {
                reason: "key name empty".into(),
            });
        }
        if raw.len() > KEY_NAME_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("key name '{raw}' exceeds {KEY_NAME_BYTES_MAX} bytes"),
            });
        }
        if !KEY_NAME_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!(
                    "key name '{raw}' must match ^[A-Za-z0-9_-][A-Za-z0-9_.-]{{0,127}}$"
                ),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for KeyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct PublicKey(String);

impl PublicKey {
    /// Wrap an openssh-format public key string. Empty input is rejected;
    /// further validation is deferred to consumers that parse with `ssh-key`.
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(Error::ConfigInvalid {
                reason: "public key empty".into(),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub name: KeyName,
    pub public: PublicKey,
    pub private: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Construct an EC2-style MD5 fingerprint string: 16 colon-separated
    /// lowercase hex bytes (`aa:bb:cc:...`). Used to compare local key with
    /// what AWS returned. Trusted EC2 responses may bypass validation via
    /// [`Fingerprint::from_trusted`].
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !FINGERPRINT_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!(
                    "fingerprint '{raw}' must be 16 colon-separated lowercase hex bytes"
                ),
            });
        }
        Ok(Self(raw))
    }

    /// Construct from a string we trust (e.g. raw EC2 SDK response). Skips
    /// the format check so an adapter never silently drops a slightly-off
    /// fingerprint string — equality comparison will still surface mismatch.
    #[must_use]
    pub fn from_trusted(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        assert!(KeyName::new("devbox").is_ok());
        assert!(KeyName::new("kleya-default").is_ok());
        assert!(KeyName::new("a.b_c-1").is_ok());
    }

    #[test]
    fn rejects_path_traversal_chars() {
        assert!(KeyName::new("../oops").is_err());
        assert!(KeyName::new("foo/bar").is_err());
        assert!(KeyName::new("foo bar").is_err());
    }

    #[test]
    fn rejects_leading_dot() {
        assert!(KeyName::new(".").is_err());
        assert!(KeyName::new("..").is_err());
        assert!(KeyName::new(".hidden").is_err());
    }

    #[test]
    fn fingerprint_accepts_canonical() {
        assert!(Fingerprint::new("aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99").is_ok());
    }

    #[test]
    fn fingerprint_rejects_garbage() {
        assert!(Fingerprint::new("not-a-fingerprint").is_err());
        assert!(Fingerprint::new("AA:BB").is_err());
    }
}
