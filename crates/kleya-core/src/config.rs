#![allow(missing_docs)]

use crate::error::{Error, Result};
use crate::limits::{TAGS_PER_TEMPLATE_MAX, TEMPLATES_COUNT_MAX};
use crate::model::{key::KeyName, region::Region, tag::Tag};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_region")]
    pub default_region: String,
    #[serde(default = "default_profile")]
    pub default_profile: String,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub bootstrap: BootstrapCfg,
    #[serde(default)]
    pub ssh: SshCfg,
    #[serde(default)]
    pub keys: KeysCfg,
    #[serde(default)]
    pub templates: Vec<TemplateCfg>,
}

fn default_region() -> String {
    "eu-west-1".into()
}
fn default_profile() -> String {
    "default".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "default_instance_type")]
    pub instance_type: String,
    #[serde(default = "default_market")]
    pub market: String,
    #[serde(default = "default_spot_type")]
    pub spot_type: String,
    #[serde(default = "default_ami_alias")]
    pub ami_alias: String,
}
fn default_instance_type() -> String {
    "m8g.xlarge".into()
}
fn default_market() -> String {
    "spot".into()
}
fn default_spot_type() -> String {
    "one-time".into()
}
fn default_ami_alias() -> String {
    "amazon-linux-2023-arm64".into()
}
impl Default for Defaults {
    fn default() -> Self {
        Self {
            instance_type: default_instance_type(),
            market: default_market(),
            spot_type: default_spot_type(),
            ami_alias: default_ami_alias(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BootstrapCfg {
    #[serde(default)]
    pub user_data_path: Option<String>,
    #[serde(default = "yes")]
    pub install_ghostty_terminfo: bool,
}
fn yes() -> bool {
    true
}
impl Default for BootstrapCfg {
    fn default() -> Self {
        Self {
            user_data_path: None,
            install_ghostty_terminfo: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SshCfg {
    #[serde(default = "default_ssh_user")]
    pub user: String,
    #[serde(default = "yes")]
    pub tmux: bool,
    #[serde(default = "default_session")]
    pub tmux_session: String,
    #[serde(default)]
    pub extra_args: Vec<String>,
}
fn default_ssh_user() -> String {
    "ec2-user".into()
}
fn default_session() -> String {
    "kleya".into()
}
impl Default for SshCfg {
    fn default() -> Self {
        Self {
            user: default_ssh_user(),
            tmux: true,
            tmux_session: default_session(),
            extra_args: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeysCfg {
    #[serde(default = "default_keys_dir")]
    pub dir: String,
    #[serde(default = "default_key_name")]
    pub default_key_name: String,
}
fn default_keys_dir() -> String {
    "~/.config/kleya/keys".into()
}
fn default_key_name() -> String {
    "kleya-default".into()
}
impl Default for KeysCfg {
    fn default() -> Self {
        Self {
            dir: default_keys_dir(),
            default_key_name: default_key_name(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TemplateCfg {
    pub name: String,
    pub ami_id: Option<String>,
    pub instance_type: Option<String>,
    pub key_name: Option<String>,
    pub security_group_ids: Option<Vec<String>>,
    pub subnet_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagCfg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TagCfg {
    pub key: String,
    pub value: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_region: default_region(),
            default_profile: default_profile(),
            defaults: Defaults::default(),
            bootstrap: BootstrapCfg::default(),
            ssh: SshCfg::default(),
            keys: KeysCfg::default(),
            templates: vec![],
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        Region::new(&self.default_region)?;
        KeyName::new(&self.keys.default_key_name)?;
        if self.templates.len() > TEMPLATES_COUNT_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!(
                    "too many templates: {} > {TEMPLATES_COUNT_MAX}",
                    self.templates.len()
                ),
            });
        }
        for t in &self.templates {
            if let Some(k) = &t.key_name {
                KeyName::new(k)?;
            }
            if t.tags.len() > TAGS_PER_TEMPLATE_MAX {
                return Err(Error::ConfigInvalid {
                    reason: format!(
                        "template '{}' has {} tags > {TAGS_PER_TEMPLATE_MAX}",
                        t.name,
                        t.tags.len()
                    ),
                });
            }
            for tag in &t.tags {
                Tag::new(&tag.key, &tag.value)?;
            }
        }
        match self.defaults.market.as_str() {
            "spot" | "on-demand" => {}
            other => {
                return Err(Error::ConfigInvalid {
                    reason: format!("defaults.market must be spot|on-demand (got '{other}')"),
                });
            }
        }
        match self.defaults.spot_type.as_str() {
            "one-time" | "persistent" => {}
            other => {
                return Err(Error::ConfigInvalid {
                    reason: format!(
                        "defaults.spot_type must be one-time|persistent (got '{other}')"
                    ),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        Config::default().validate().expect("defaults are valid");
    }

    #[test]
    fn rejects_bad_region() {
        let mut c = Config::default();
        c.default_region = "not a region".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_excess_templates() {
        let mut c = Config::default();
        c.templates = (0..=TEMPLATES_COUNT_MAX)
            .map(|i| TemplateCfg {
                name: format!("t{i}"),
                ami_id: None,
                instance_type: None,
                key_name: None,
                security_group_ids: None,
                subnet_id: None,
                tags: vec![],
            })
            .collect();
        assert!(c.validate().is_err());
    }

    #[test]
    fn rejects_unknown_market() {
        let mut c = Config::default();
        c.defaults.market = "lottery".into();
        assert!(c.validate().is_err());
    }
}
