use kleya_core::{Config, Result};

pub fn load(path: Option<&str>) -> Result<Config> {
    let _ = path;
    let cfg = Config::default();
    cfg.validate()?;
    Ok(cfg)
}

#[must_use]
pub fn resolved_path(path: Option<&str>) -> Option<String> {
    path.map(str::to_string)
}
