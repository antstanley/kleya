use std::fs;
use std::path::{Path, PathBuf};

use kleya_core::limits::CONFIG_BYTES_MAX;
use kleya_core::{Config, Error, Result};

pub fn load(explicit: Option<&str>) -> Result<Config> {
    let path = resolved_path(explicit);
    let Some(path) = path else {
        let cfg = Config::default();
        cfg.validate()?;
        return Ok(cfg);
    };
    let p = PathBuf::from(shellexpand::tilde(&path).to_string());
    let bytes = fs::read(&p)?;
    if bytes.len() > CONFIG_BYTES_MAX {
        return Err(Error::ConfigInvalid {
            reason: format!("config file {} bytes > {CONFIG_BYTES_MAX}", bytes.len()),
        });
    }
    let text = String::from_utf8(bytes).map_err(|e| Error::ConfigInvalid {
        reason: format!("config not utf-8: {e}"),
    })?;
    let cfg = parse_by_ext(&p, &text)?;
    cfg.validate()?;
    Ok(cfg)
}

fn parse_by_ext(path: &Path, text: &str) -> Result<Config> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "toml" => toml::from_str(text).map_err(map_serde),
        "yaml" | "yml" => serde_yaml::from_str(text).map_err(map_serde),
        "json" => serde_json::from_str(text).map_err(map_serde),
        "jsonc" => {
            let json =
                jsonc_parser::parse_to_serde_value(text, &jsonc_parser::ParseOptions::default())
                    .map_err(|e| Error::ConfigInvalid {
                        reason: format!("jsonc: {e}"),
                    })?
                    .ok_or_else(|| Error::ConfigInvalid {
                        reason: "jsonc empty".into(),
                    })?;
            serde_json::from_value(json).map_err(map_serde)
        }
        other => Err(Error::ConfigInvalid {
            reason: format!("unknown config extension: {other}"),
        }),
    }
}

fn map_serde<E: std::fmt::Display>(e: E) -> Error {
    Error::ConfigInvalid {
        reason: format!("parse: {e}"),
    }
}

#[must_use]
pub fn resolved_path(explicit: Option<&str>) -> Option<String> {
    if let Some(p) = explicit {
        return Some(p.to_string());
    }
    let home = std::env::var("HOME").ok()?;
    for ext in ["toml", "yaml", "yml", "json", "jsonc"] {
        let p = format!("{home}/.config/kleya/config.{ext}");
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    None
}
