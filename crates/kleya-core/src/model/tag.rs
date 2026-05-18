use crate::error::{Error, Result};
use crate::limits::{TAG_KEY_BYTES_MAX, TAG_VALUE_BYTES_MAX};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

impl Tag {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        let key = key.into();
        let value = value.into();
        if key.is_empty() {
            return Err(Error::ConfigInvalid {
                reason: "tag key empty".into(),
            });
        }
        if key.len() > TAG_KEY_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("tag key '{key}' exceeds {TAG_KEY_BYTES_MAX} bytes"),
            });
        }
        if value.len() > TAG_VALUE_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("tag value for '{key}' exceeds {TAG_VALUE_BYTES_MAX} bytes"),
            });
        }
        Ok(Self { key, value })
    }
}

pub const KLEYA_TAG_MANAGED: &str = "kleya:managed";
pub const KLEYA_TAG_TEMPLATE: &str = "kleya:template";
pub const KLEYA_TAG_KEY: &str = "kleya:key";
pub const KLEYA_TAG_NAME: &str = "Name";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_key() {
        assert!(Tag::new("", "v").is_err());
    }

    #[test]
    fn rejects_oversize_key() {
        let key = "k".repeat(TAG_KEY_BYTES_MAX + 1);
        assert!(Tag::new(key, "v").is_err());
    }

    #[test]
    fn accepts_at_limit() {
        let key = "k".repeat(TAG_KEY_BYTES_MAX);
        let val = "v".repeat(TAG_VALUE_BYTES_MAX);
        assert!(Tag::new(key, val).is_ok());
    }
}
