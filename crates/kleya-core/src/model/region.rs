#![allow(clippy::expect_used, clippy::disallowed_methods)]

use crate::error::{Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static REGION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z]{2}-[a-z]+-[0-9]+$").expect("static regex compiles"));

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Region(String);
impl Region {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !REGION_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("region '{raw}' invalid (e.g. eu-west-1)"),
            });
        }
        Ok(Self(raw))
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AmiId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubnetId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecurityGroupId(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_eu_west_1() {
        assert!(Region::new("eu-west-1").is_ok());
    }
    #[test]
    fn rejects_garbage() {
        assert!(Region::new("eu west 1").is_err());
    }
}
