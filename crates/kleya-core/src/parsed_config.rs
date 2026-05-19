//! Typed, parsed view of [`Config`] — the form commands receive after
//! validation.
//!
//! The serde-facing [`Config`] is intentionally stringly-typed so it can
//! deserialize from any of the supported formats (TOML, YAML, JSON, JSONC).
//! Validation lives in [`Config::validate`] but throws away the proofs;
//! downstream code then re-parses the same values (and can drift from the
//! validator). Parsing once at load time into [`ParsedConfig`] removes that
//! whole class of duplication.

#![allow(missing_docs)]

use std::path::PathBuf;

use crate::config::{Config, SshCfg, TagCfg, TemplateCfg};
use crate::error::{Error, Result};
use crate::model::{
    key::KeyName,
    market::{MarketKind, SpotType},
    region::{AmiId, Region, SecurityGroupId, SubnetId},
    tag::Tag,
    template::TemplateName,
};

/// Typed configuration view. Construct via [`Config::parse`].
#[derive(Debug, Clone)]
pub struct ParsedConfig {
    pub default_region: Region,
    pub default_profile: String,
    pub default_key_name: KeyName,
    pub default_instance_type: String,
    pub default_market: MarketKind,
    pub default_spot_type: SpotType,
    pub default_ami_alias: String,
    pub bootstrap: ParsedBootstrap,
    pub ssh: SshCfg,
    pub keys_dir: PathBuf,
    pub templates: Vec<ParsedTemplate>,
    /// Original deserialized config — retained so `kleya config show` can
    /// serialize back to TOML without re-stringifying typed values.
    pub raw: Config,
}

#[derive(Debug, Clone)]
pub struct ParsedBootstrap {
    /// User-data override file path with `~/` expanded against `$HOME`.
    pub user_data_path: Option<PathBuf>,
    pub install_ghostty_terminfo: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedTemplate {
    pub name: TemplateName,
    pub ami_id: Option<AmiId>,
    pub instance_type: Option<String>,
    pub key_name: Option<KeyName>,
    pub security_group_ids: Vec<SecurityGroupId>,
    pub subnet_id: Option<SubnetId>,
    pub tags: Vec<Tag>,
}

impl Config {
    /// Validate the deserialized config and produce a typed view. This is the
    /// only place market/spot-type strings or path-like fields are parsed.
    pub fn parse(self) -> Result<ParsedConfig> {
        self.validate()?;
        let raw = self.clone();
        let Config {
            default_region,
            default_profile,
            defaults,
            bootstrap,
            ssh,
            keys,
            templates,
        } = self;
        let default_region = Region::new(default_region)?;
        let default_key_name = KeyName::new(keys.default_key_name.clone())?;
        let default_market = parse_market(&defaults.market)?;
        let default_spot_type = parse_spot_type(&defaults.spot_type)?;
        let keys_dir = expand_tilde(&keys.dir);
        let bootstrap = ParsedBootstrap {
            user_data_path: bootstrap.user_data_path.as_deref().map(expand_tilde),
            install_ghostty_terminfo: bootstrap.install_ghostty_terminfo,
        };
        let templates = templates
            .into_iter()
            .map(parse_template)
            .collect::<Result<Vec<_>>>()?;
        Ok(ParsedConfig {
            default_region,
            default_profile,
            default_key_name,
            default_instance_type: defaults.instance_type,
            default_market,
            default_spot_type,
            default_ami_alias: defaults.ami_alias,
            bootstrap,
            ssh,
            keys_dir,
            templates,
            raw,
        })
    }
}

fn parse_market(s: &str) -> Result<MarketKind> {
    match s {
        "spot" => Ok(MarketKind::Spot),
        "on-demand" => Ok(MarketKind::OnDemand),
        other => Err(Error::ConfigInvalid {
            reason: format!("defaults.market must be spot|on-demand (got '{other}')"),
        }),
    }
}

fn parse_spot_type(s: &str) -> Result<SpotType> {
    match s {
        "one-time" => Ok(SpotType::OneTime),
        "persistent" => Ok(SpotType::Persistent),
        other => Err(Error::ConfigInvalid {
            reason: format!("defaults.spot_type must be one-time|persistent (got '{other}')"),
        }),
    }
}

fn parse_template(t: TemplateCfg) -> Result<ParsedTemplate> {
    let name = TemplateName::new(&t.name)?;
    let ami_id = t.ami_id.map(AmiId::new).transpose()?;
    let key_name = t.key_name.map(KeyName::new).transpose()?;
    let security_group_ids = t
        .security_group_ids
        .unwrap_or_default()
        .into_iter()
        .map(SecurityGroupId::new)
        .collect::<Result<Vec<_>>>()?;
    let subnet_id = t.subnet_id.map(SubnetId::new).transpose()?;
    let tags = t
        .tags
        .into_iter()
        .map(|TagCfg { key, value }| Tag::new(key, value))
        .collect::<Result<Vec<_>>>()?;
    Ok(ParsedTemplate {
        name,
        ami_id,
        instance_type: t.instance_type,
        key_name,
        security_group_ids,
        subnet_id,
        tags,
    })
}

/// Expand a leading `~/` against `$HOME`. A bare `~` and `$HOME` not set are
/// left as-is.
fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

impl ParsedConfig {
    /// Locate a template by name, returning `None` if absent.
    #[must_use]
    pub fn template(&self, name: &TemplateName) -> Option<&ParsedTemplate> {
        self.templates.iter().find(|t| &t.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn defaults_parse() {
        let p = Config::default().parse().expect("defaults parse");
        assert_eq!(p.default_region.as_str(), "eu-west-1");
        assert_eq!(p.default_market, MarketKind::Spot);
        assert_eq!(p.default_spot_type, SpotType::OneTime);
        assert_eq!(p.default_key_name.as_str(), "kleya-default");
    }

    #[test]
    fn bad_market_rejected() {
        let mut c = Config::default();
        c.defaults.market = "lottery".into();
        assert!(c.parse().is_err());
    }

    #[test]
    fn template_with_bad_ami_rejected() {
        let mut c = Config::default();
        c.templates.push(crate::config::TemplateCfg {
            name: "devbox".into(),
            ami_id: Some("not-an-ami".into()),
            instance_type: None,
            key_name: None,
            security_group_ids: None,
            subnet_id: None,
            tags: vec![],
        });
        assert!(c.parse().is_err());
    }

    #[test]
    fn tilde_expansion_uses_home() {
        let prev = std::env::var("HOME").ok();
        std::env::set_var("HOME", "/tmp/home");
        let mut c = Config::default();
        c.keys.dir = "~/.config/kleya/keys".into();
        let p = c.parse().expect("parse");
        assert_eq!(
            p.keys_dir.display().to_string(),
            "/tmp/home/.config/kleya/keys"
        );
        if let Some(h) = prev {
            std::env::set_var("HOME", h);
        }
    }
}
