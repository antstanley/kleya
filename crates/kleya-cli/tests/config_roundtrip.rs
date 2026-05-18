#![allow(missing_docs, clippy::unwrap_used)]

use kleya_core::Config;

#[test]
fn defaults_serialize_in_all_formats_and_reparse_equal() {
    let cfg = Config::default();
    let toml_text = toml::to_string(&cfg).expect("toml");
    let yaml_text = serde_yaml::to_string(&cfg).expect("yaml");
    let json_text = serde_json::to_string(&cfg).expect("json");

    let toml_back: Config = toml::from_str(&toml_text).expect("toml parse");
    let yaml_back: Config = serde_yaml::from_str(&yaml_text).expect("yaml parse");
    let json_back: Config = serde_json::from_str(&json_text).expect("json parse");

    assert_eq!(cfg, toml_back);
    assert_eq!(cfg, yaml_back);
    assert_eq!(cfg, json_back);
}

#[test]
fn jsonc_with_comments_parses() {
    let jsonc = r#"
        {
            // comment
            "default_region": "us-east-1",
            "default_profile": "default"
        }
    "#;
    let v = jsonc_parser::parse_to_serde_value(jsonc, &jsonc_parser::ParseOptions::default())
        .expect("jsonc")
        .expect("value");
    let c: Config = serde_json::from_value(v).expect("config");
    assert_eq!(c.default_region, "us-east-1");
}
