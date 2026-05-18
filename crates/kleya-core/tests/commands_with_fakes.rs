#![allow(missing_docs, clippy::unwrap_used, clippy::items_after_statements)]

use std::sync::Arc;

use kleya_core::commands::launch::{LaunchOpts, LaunchService};
use kleya_core::commands::template::TemplateService;
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
        name: TemplateName(name.into()),
        ami_id: Some(AmiId("ami-1".into())),
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
        config: Arc::new(Config::default()),
    };
    svc.create(sample_spec("devbox")).await.expect("create");
    svc.create(sample_spec("workbox")).await.expect("create");
    let listed = svc.list().await.expect("list");
    assert_eq!(listed.len(), 2);

    svc.delete_by_name(&TemplateName("devbox".into()))
        .await
        .expect("delete");
    let listed = svc.list().await.expect("list");
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn delete_unknown_returns_error() {
    let svc = TemplateService {
        compute: Arc::new(InMemoryCompute::new()),
        config: Arc::new(Config::default()),
    };
    let err = svc
        .delete_by_name(&TemplateName("ghost".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, kleya_core::Error::ConfigInvalid { .. }));
}

#[tokio::test]
async fn launch_zero_config_creates_default_template_and_instance() {
    let svc = LaunchService {
        compute: Arc::new(InMemoryCompute::new()),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen: Arc::new(FakeIdGen::new()),
        config: Arc::new(Config::default()),
        bootstrap_tpl: "echo hi",
        ghostty_tinfo: "",
    };
    let res = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: None,
            dry_run: false,
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
        config: Arc::new(Config::default()),
        bootstrap_tpl: "echo hi",
        ghostty_tinfo: "",
    };
    let res = svc
        .run(LaunchOpts {
            template_name: None,
            instance_name: None,
            dry_run: true,
        })
        .await
        .expect("dry-run ok");
    assert!(res.is_none());
    use kleya_core::ports::cloud_compute::CloudCompute;
    assert!(compute.template_list().await.unwrap().is_empty());
}
