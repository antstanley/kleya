#![allow(missing_docs, clippy::unwrap_used, clippy::items_after_statements)]

use std::sync::Arc;

use kleya_core::commands::launch::{LaunchOpts, LaunchService};
use kleya_core::commands::list::ListService;
use kleya_core::commands::template::TemplateService;
use kleya_core::commands::terminate::TerminateService;
use kleya_core::config::Config;
use kleya_core::model::{
    key::KeyName,
    market::{MarketKind, SpotType},
    region::AmiId,
    template::{TemplateName, TemplateSpec},
};
use kleya_core::test_support::{FakeIdGen, InMemoryCompute, InMemoryKeyStore};

fn sample_spec(name: &str) -> TemplateSpec {
    TemplateSpec {
        name: TemplateName::new(name).unwrap(),
        ami_id: Some(AmiId::new("ami-deadbeef").unwrap()),
        ami_alias: None,
        instance_type: "m8g.xlarge".into(),
        key_name: KeyName::new("kleya-default").unwrap(),
        security_group_ids: vec![],
        subnet_id: None,
        market: MarketKind::Spot,
        spot_type: SpotType::OneTime,
        tags: vec![],
        user_data_base64: "H4sIAAAA".into(),
    }
}

#[tokio::test]
async fn create_then_list_then_delete() {
    let svc = TemplateService {
        compute: Arc::new(InMemoryCompute::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    svc.create(sample_spec("devbox")).await.expect("create");
    svc.create(sample_spec("workbox")).await.expect("create");
    let listed = svc.list().await.expect("list");
    assert_eq!(listed.len(), 2);

    svc.delete_by_name(&TemplateName::new("devbox").unwrap())
        .await
        .expect("delete");
    let listed = svc.list().await.expect("list");
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn delete_unknown_returns_error() {
    let svc = TemplateService {
        compute: Arc::new(InMemoryCompute::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    let err = svc
        .delete_by_name(&TemplateName::new("ghost").unwrap())
        .await
        .unwrap_err();
    assert!(matches!(err, kleya_core::Error::TemplateNotFound { .. }));
}

#[tokio::test]
async fn launch_zero_config_creates_default_template_and_instance() {
    let svc = LaunchService {
        compute: Arc::new(InMemoryCompute::new()),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    let res = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: None,
            instance_type: None,
            market: None,
            dry_run: false,
            regenerate_key: false,
            cancel: None,
        })
        .await
        .expect("launch ok");
    let inst = res.expect("returned instance");
    assert!(matches!(
        inst.state,
        kleya_core::model::instance::InstanceState::Running
    ));
}

#[tokio::test]
async fn launch_dry_run_returns_none_and_does_not_create_template() {
    let compute = Arc::new(InMemoryCompute::new());
    let svc = LaunchService {
        compute: compute.clone(),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    let res = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: None,
            instance_type: None,
            market: None,
            dry_run: true,
            regenerate_key: false,
            cancel: None,
        })
        .await
        .expect("dry-run ok");
    assert!(res.is_none());
    use kleya_core::ports::cloud_compute::CloudCompute;
    assert!(compute.template_list().await.unwrap().is_empty());
}

#[tokio::test]
async fn terminate_by_name_succeeds_when_unique() {
    let compute = Arc::new(InMemoryCompute::new());
    let svc = LaunchService {
        compute: compute.clone(),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    let inst = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: Some("solo".into()),
            instance_type: None,
            market: None,
            dry_run: false,
            regenerate_key: false,
            cancel: None,
        })
        .await
        .expect("launch")
        .expect("inst");

    let term = TerminateService {
        compute: compute.clone(),
        region: "eu-west-1".into(),
    };
    let id = term.terminate_by_handle("solo").await.expect("terminate");
    assert_eq!(id, inst.id);
}

#[tokio::test]
async fn terminate_unknown_returns_not_found() {
    let compute = Arc::new(InMemoryCompute::new());
    let term = TerminateService {
        compute,
        region: "eu-west-1".into(),
    };
    let err = term.terminate_by_handle("ghost").await.unwrap_err();
    assert!(matches!(err, kleya_core::Error::InstanceNotFound { .. }));
}

#[tokio::test]
async fn list_returns_only_managed() {
    let compute = Arc::new(InMemoryCompute::new());
    let svc = LaunchService {
        compute: compute.clone(),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    svc.run(LaunchOpts {
        template_name: None,
        instance_name: Some("a".into()),
        instance_type: None,
        market: None,
        dry_run: false,
        regenerate_key: false,
        cancel: None,
    })
    .await
    .unwrap();
    svc.run(LaunchOpts {
        template_name: None,
        instance_name: Some("b".into()),
        instance_type: None,
        market: None,
        dry_run: false,
        regenerate_key: false,
        cancel: None,
    })
    .await
    .unwrap();
    let list = ListService { compute }.list_managed().await.expect("list");
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn launch_regenerates_orphaned_key_when_flag_set() {
    use kleya_core::model::key::PublicKey;
    use kleya_core::ports::cloud_compute::CloudCompute;

    let compute = Arc::new(InMemoryCompute::new());
    let key_store = Arc::new(InMemoryKeyStore::new());
    let name = KeyName::new("kleya-default").unwrap();
    compute
        .ensure_default_keypair(&name, &PublicKey::new("ssh-ed25519 AAAA seed").unwrap())
        .await
        .unwrap();
    assert!(!kleya_core::ports::key_store::KeyStore::exists(
        key_store.as_ref(),
        &name
    ));
    let before = compute
        .keypair_fingerprint(&name)
        .await
        .unwrap()
        .expect("seeded fingerprint");

    let svc = LaunchService {
        compute: compute.clone(),
        key_store: key_store.clone(),
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    let out = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: Some("solo".into()),
            instance_type: None,
            market: None,
            dry_run: false,
            regenerate_key: true,
            cancel: None,
        })
        .await
        .expect("launch should succeed");
    assert!(out.is_some(), "instance returned");

    assert!(kleya_core::ports::key_store::KeyStore::exists(
        key_store.as_ref(),
        &name
    ));
    let after = compute
        .keypair_fingerprint(&name)
        .await
        .unwrap()
        .expect("post-regenerate fingerprint");
    assert_ne!(before, after, "fingerprint must change after regenerate");
}

#[tokio::test]
async fn launch_errors_on_orphaned_key_when_flag_unset() {
    use kleya_core::model::key::PublicKey;
    use kleya_core::ports::cloud_compute::CloudCompute;

    let compute = Arc::new(InMemoryCompute::new());
    let key_store = Arc::new(InMemoryKeyStore::new());
    let name = KeyName::new("kleya-default").unwrap();
    compute
        .ensure_default_keypair(&name, &PublicKey::new("ssh-ed25519 AAAA seed").unwrap())
        .await
        .unwrap();

    let svc = LaunchService {
        compute,
        key_store,
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default().parse().unwrap()),
    };
    let err = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: None,
            instance_type: None,
            market: None,
            dry_run: false,
            regenerate_key: false,
            cancel: None,
        })
        .await
        .expect_err("must fail");
    assert!(
        matches!(err, kleya_core::Error::KeyOrphaned { .. }),
        "got: {err:?}"
    );
}

#[test]
fn in_memory_compute_declares_fake_md5_fingerprint_algorithm() {
    use kleya_core::ports::cloud_compute::CloudCompute;
    assert_eq!(InMemoryCompute::new().fingerprint_algorithm(), "fake-md5");
}
