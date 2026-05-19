//! Launch template newtypes — validated at construction.

#![allow(missing_docs)]

use crate::error::{Error, Result};
use crate::model::{
    key::KeyName,
    market::{MarketKind, SpotType},
    region::{AmiId, SecurityGroupId, SubnetId},
    tag::Tag,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static TEMPLATE_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z0-9._\-]{1,128}$").expect("static regex compiles"));

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static TEMPLATE_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^lt-[0-9a-f]{8,32}$").expect("static regex compiles"));

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId(String);

impl TemplateId {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !TEMPLATE_ID_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("template id '{raw}' must match ^lt-[0-9a-f]+$"),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateName(String);

impl TemplateName {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !TEMPLATE_NAME_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("template name '{raw}' must match ^[A-Za-z0-9._-]{{1,128}}$"),
            });
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TemplateName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct TemplateSpec {
    pub name: TemplateName,
    pub ami_id: Option<AmiId>,
    pub ami_alias: Option<String>,
    pub instance_type: String,
    pub key_name: KeyName,
    pub security_group_ids: Vec<SecurityGroupId>,
    pub subnet_id: Option<SubnetId>,
    pub market: MarketKind,
    pub spot_type: SpotType,
    pub tags: Vec<Tag>,
    pub user_data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVersion(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: TemplateId,
    pub name: TemplateName,
    pub latest_version: TemplateVersion,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_accepts_short_handles() {
        assert!(TemplateName::new("default").is_ok());
        assert!(TemplateName::new("kleya-dev").is_ok());
        assert!(TemplateName::new("a.b_c-1").is_ok());
    }

    #[test]
    fn name_rejects_empty_and_bad_chars() {
        assert!(TemplateName::new("").is_err());
        assert!(TemplateName::new("with space").is_err());
        assert!(TemplateName::new("slash/in/name").is_err());
    }

    #[test]
    fn id_accepts_canonical() {
        assert!(TemplateId::new("lt-0123456789abcdef0").is_ok());
        assert!(TemplateId::new("lt-deadbeef").is_ok());
        assert!(TemplateId::new("not-an-id").is_err());
    }
}
