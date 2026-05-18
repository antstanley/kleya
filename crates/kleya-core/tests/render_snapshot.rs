#![allow(missing_docs)]

use kleya_core::bootstrap::render::{render, render_with, BootstrapVars};

#[test]
fn renders_real_template_with_ghostty() {
    let vars = BootstrapVars::default_with(kleya_bootstrap_assets::GHOSTTY_TERMINFO);
    let out = render(&vars).expect("renders");
    insta::assert_snapshot!("setup_devbox_default", out);
}

#[test]
fn renders_real_template_without_ghostty() {
    let mut vars = BootstrapVars::default_with("");
    vars.install_ghostty_terminfo = false;
    let out = render_with(kleya_bootstrap_assets::SETUP_TEMPLATE, &vars).expect("renders");
    insta::assert_snapshot!("setup_devbox_no_ghostty", out);
}
