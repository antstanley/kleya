use crate::error::{Error, Result};
use minijinja::{context, Environment};

pub struct BootstrapVars<'a> {
    pub install_ghostty_terminfo: bool,
    pub ghostty_terminfo_source: &'a str,
    pub install_dev_tools: bool,
    pub node_major: u8,
    pub python_version: &'a str,
    pub extra_pre_lines: &'a [String],
    pub extra_post_lines: &'a [String],
}

impl<'a> BootstrapVars<'a> {
    #[must_use]
    pub fn default_with(ghostty_terminfo_source: &'a str) -> Self {
        Self {
            install_ghostty_terminfo: true,
            ghostty_terminfo_source,
            install_dev_tools: false,
            node_major: 24,
            python_version: "3.12",
            extra_pre_lines: &[],
            extra_post_lines: &[],
        }
    }
}

pub fn render(vars: &BootstrapVars<'_>) -> Result<String> {
    render_with(kleya_bootstrap_assets::SETUP_TEMPLATE, vars)
}

pub fn render_with(template: &str, vars: &BootstrapVars<'_>) -> Result<String> {
    assert!(!template.is_empty(), "bootstrap template empty");
    assert!(vars.node_major >= 18, "node_major too low");
    let mut env = Environment::new();
    env.add_template("setup", template)
        .map_err(|e| Error::ConfigInvalid {
            reason: format!("template render: {e}"),
        })?;
    let tpl = env
        .get_template("setup")
        .map_err(|e| Error::ConfigInvalid {
            reason: format!("template render: {e}"),
        })?;
    let out = tpl
        .render(context! {
            install_ghostty_terminfo => vars.install_ghostty_terminfo,
            ghostty_terminfo_source  => vars.ghostty_terminfo_source,
            install_dev_tools        => vars.install_dev_tools,
            node_major               => vars.node_major,
            python_version           => vars.python_version,
            extra_pre_lines          => vars.extra_pre_lines,
            extra_post_lines         => vars.extra_post_lines,
        })
        .map_err(|e| Error::ConfigInvalid {
            reason: format!("template render: {e}"),
        })?;
    assert!(!out.is_empty(), "rendered output empty");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "set -e\n\
                     {% if install_ghostty_terminfo %}GHOSTTY{% endif %}\n\
                     node{{ node_major }} py{{ python_version }}\n";

    #[test]
    fn renders_with_ghostty_block() {
        let v = BootstrapVars {
            install_ghostty_terminfo: true,
            ghostty_terminfo_source: "TERMINFO",
            install_dev_tools: false,
            node_major: 24,
            python_version: "3.12",
            extra_pre_lines: &[],
            extra_post_lines: &[],
        };
        let out = render_with(T, &v).expect("renders");
        assert!(out.contains("GHOSTTY"));
        assert!(out.contains("node24"));
        assert!(out.contains("py3.12"));
    }

    #[test]
    fn omits_ghostty_block_when_disabled() {
        let v = BootstrapVars {
            install_ghostty_terminfo: false,
            ghostty_terminfo_source: "",
            install_dev_tools: false,
            node_major: 24,
            python_version: "3.12",
            extra_pre_lines: &[],
            extra_post_lines: &[],
        };
        let out = render_with(T, &v).expect("renders");
        assert!(!out.contains("GHOSTTY"));
    }

    #[test]
    fn default_render_uses_embedded_template() {
        let v = BootstrapVars::default_with(kleya_bootstrap_assets::GHOSTTY_TERMINFO);
        let out = render(&v).expect("renders");
        assert!(out.contains("set -euxo pipefail"));
    }
}
