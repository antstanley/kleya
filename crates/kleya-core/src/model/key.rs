#![allow(clippy::expect_used, clippy::disallowed_methods)]

use crate::error::{Error, Result};
use crate::limits::KEY_NAME_BYTES_MAX;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static KEY_NAME_RE: Lazy<Regex> = Lazy::new(|| {
    // Leading char excludes '.' to prevent hidden-file or traversal segments.
    Regex::new(r"^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,127}$").expect("static regex compiles")
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
pub struct PublicKey(pub String);

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub name: KeyName,
    pub public: PublicKey,
    pub private: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(pub String);

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
}
