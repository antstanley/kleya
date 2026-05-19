#![allow(missing_docs, clippy::unwrap_used)]

use kleya_core::config::KeysCfg;
use kleya_core::model::key::KeyName;
use kleya_core::ports::key_store::KeyStore;

#[test]
fn generate_then_read_and_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = KeysCfg {
        dir: tmp.path().display().to_string(),
        default_key_name: "k".into(),
    };
    let store = kleya_cli::key_store_fs::FsKeyStore::from_config(&cfg).unwrap();
    let name = KeyName::new("k").unwrap();
    store.generate(&name).expect("generate");
    let _pub_text = store.read_public(&name).expect("read");
    let path = store.private_path(&name).expect("path");
    assert!(path.exists());
}

#[test]
fn private_path_errors_when_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = KeysCfg {
        dir: tmp.path().display().to_string(),
        default_key_name: "k".into(),
    };
    let store = kleya_cli::key_store_fs::FsKeyStore::from_config(&cfg).unwrap();
    let name = KeyName::new("absent").unwrap();
    let err = store.private_path(&name).unwrap_err();
    assert!(matches!(err, kleya_core::Error::KeyOrphaned { .. }));
}

#[test]
fn fingerprint_is_deterministic_and_well_formatted() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = KeysCfg {
        dir: tmp.path().display().to_string(),
        default_key_name: "k".into(),
    };
    let store = kleya_cli::key_store_fs::FsKeyStore::from_config(&cfg).unwrap();
    let name = KeyName::new("k").unwrap();
    store.generate(&name).unwrap();
    let fp1 = store.fingerprint(&name).unwrap();
    let fp2 = store.fingerprint(&name).unwrap();
    assert_eq!(fp1, fp2, "fingerprint must be deterministic");
    let s = fp1.as_str();
    assert_eq!(s.len(), 47, "aa:bb:... format is 47 chars");
    assert!(s.chars().filter(|c| *c == ':').count() == 15);
}
