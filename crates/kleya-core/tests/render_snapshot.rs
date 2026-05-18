#![allow(missing_docs)]

use kleya_core::bootstrap::render::{render, BootstrapVars};

#[test]
fn renders_real_template_with_ghostty() {
    let template = kleya_bootstrap_assets::SETUP_TEMPLATE;
    let ghostty = kleya_bootstrap_assets::GHOSTTY_TERMINFO;
    let vars = BootstrapVars::default_with(ghostty);
    let out = render(template, &vars).expect("renders");
    insta::assert_snapshot!("setup_devbox_default", out);
}

#[test]
fn renders_real_template_without_ghostty() {
    let template = kleya_bootstrap_assets::SETUP_TEMPLATE;
    let mut vars = BootstrapVars::default_with("");
    vars.install_ghostty_terminfo = false;
    let out = render(template, &vars).expect("renders");
    insta::assert_snapshot!("setup_devbox_no_ghostty", out);
}
