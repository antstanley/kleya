//! Region + AWS resource id newtypes — validated at construction.

#![allow(missing_docs)]

use crate::error::{Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static REGION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z]{2}-[a-z]+-[0-9]+$").expect("static regex compiles"));

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static AMI_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^ami-[0-9a-f]{8,17}$").expect("static regex compiles"));

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static SUBNET_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^subnet-[0-9a-f]{8,17}$").expect("static regex compiles"));

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static SG_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^sg-[0-9a-f]{8,17}$").expect("static regex compiles"));

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

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AmiId(String);

impl AmiId {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !AMI_ID_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("ami id '{raw}' must match ^ami-[0-9a-f]{{8,17}}$"),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AmiId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubnetId(String);

impl SubnetId {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !SUBNET_ID_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("subnet id '{raw}' must match ^subnet-[0-9a-f]{{8,17}}$"),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SubnetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecurityGroupId(String);

impl SecurityGroupId {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !SG_ID_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("security group id '{raw}' must match ^sg-[0-9a-f]{{8,17}}$"),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SecurityGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_accepts_eu_west_1() {
        assert!(Region::new("eu-west-1").is_ok());
    }

    #[test]
    fn region_rejects_garbage() {
        assert!(Region::new("eu west 1").is_err());
    }

    #[test]
    fn ami_id_accepts_canonical() {
        assert!(AmiId::new("ami-0123456789abcdef0").is_ok());
        assert!(AmiId::new("ami-deadbeef").is_ok());
    }

    #[test]
    fn ami_id_rejects_garbage() {
        assert!(AmiId::new("ami-").is_err());
        assert!(AmiId::new("not-an-ami").is_err());
        assert!(AmiId::new("ami-XYZ").is_err());
    }

    #[test]
    fn subnet_id_accepts_canonical() {
        assert!(SubnetId::new("subnet-0123456789abcdef0").is_ok());
        assert!(SubnetId::new("subnet-deadbeef").is_ok());
    }

    #[test]
    fn sg_id_accepts_canonical() {
        assert!(SecurityGroupId::new("sg-0123456789abcdef0").is_ok());
        assert!(SecurityGroupId::new("sg-deadbeef").is_ok());
    }
}
