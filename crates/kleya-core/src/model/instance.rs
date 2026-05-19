use crate::error::{Error, Result};
use crate::limits::INSTANCE_NAME_BYTES_MAX;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static INSTANCE_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,62}$").expect("static regex compiles"));

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static INSTANCE_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^i-[0-9a-f]{8,32}$").expect("static regex compiles"));

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceName(String);

impl InstanceName {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(Error::ConfigInvalid {
                reason: "instance name empty".into(),
            });
        }
        if raw.len() > INSTANCE_NAME_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("instance name '{raw}' exceeds {INSTANCE_NAME_BYTES_MAX} bytes"),
            });
        }
        if !INSTANCE_NAME_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("instance name '{raw}' must match ^[a-z0-9][a-z0-9-]{{0,62}}$"),
            });
        }
        assert!(raw.len() <= INSTANCE_NAME_BYTES_MAX);
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(String);

impl InstanceId {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !INSTANCE_ID_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("instance id '{raw}' must match ^i-[0-9a-f]+$"),
            });
        }
        Ok(Self(raw))
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceState {
    Pending,
    Running,
    ShuttingDown,
    Terminated,
    Stopping,
    Stopped,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    pub name: Option<InstanceName>,
    pub state: InstanceState,
    pub public_dns: Option<String>,
    pub public_ip: Option<String>,
    pub tags: Vec<crate::model::tag::Tag>,
}

#[derive(Debug, Default, Clone)]
pub struct InstanceFilter {
    pub name: Option<String>,
    pub managed_only: bool,
    pub states: Vec<InstanceState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_accepts_simple_lowercase() {
        assert!(InstanceName::new("devbox").is_ok());
        assert!(InstanceName::new("devbox-1").is_ok());
        assert!(InstanceName::new("a").is_ok());
    }

    #[test]
    fn name_rejects_uppercase_and_invalid_chars() {
        assert!(InstanceName::new("DevBox").is_err());
        assert!(InstanceName::new("dev_box").is_err());
        assert!(InstanceName::new("-devbox").is_err());
    }

    #[test]
    fn name_returns_err_on_empty_string() {
        let err = InstanceName::new("").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("empty"), "got: {msg}");
    }

    #[test]
    fn name_rejects_at_and_above_size_limit() {
        let at_limit = "a".repeat(INSTANCE_NAME_BYTES_MAX);
        let above = "a".repeat(INSTANCE_NAME_BYTES_MAX + 1);
        assert!(InstanceName::new(&at_limit).is_ok());
        assert!(InstanceName::new(&above).is_err());
    }

    #[test]
    fn id_accepts_canonical_aws_pattern_and_rejects_others() {
        assert!(InstanceId::new("i-0123456789abcdef").is_ok());
        assert!(InstanceId::new("i-deadbeef").is_ok());
        assert!(InstanceId::new("i-").is_err());
        assert!(InstanceId::new("not-an-id").is_err());
    }
}
