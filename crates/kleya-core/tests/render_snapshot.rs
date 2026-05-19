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

#[test]
fn render_then_encode_rejects_oversize_extra_post_lines() {
    use kleya_core::bootstrap::encode::encode_user_data;
    use kleya_core::limits::USER_DATA_RAW_BYTES_MAX;

    let line = "echo padding padding padding padding padding".to_string();
    let extras: Vec<String> = (0..2000).map(|_| line.clone()).collect();
    let total: usize = extras.iter().map(String::len).sum();
    assert!(
        total > USER_DATA_RAW_BYTES_MAX * 4,
        "precondition: extras total = {total} must exceed 4 * RAW_MAX",
    );

    let extras_slice: &[String] = &extras;
    let vars = BootstrapVars {
        install_ghostty_terminfo: false,
        ghostty_terminfo_source: "",
        install_dev_tools: false,
        node_major: 24,
        python_version: "3.12",
        extra_pre_lines: &[],
        extra_post_lines: extras_slice,
    };
    let rendered = render(&vars).expect("render");
    let err = encode_user_data(&rendered).expect_err("must reject oversize");
    assert!(
        matches!(err, kleya_core::Error::UserDataTooLarge { .. }),
        "got: {err:?}",
    );
}
