#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;

use kleya_core::commands::template::TemplateService;
use kleya_core::config::Config;
use kleya_core::model::{
    key::KeyName,
    market::{MarketKind, SpotType},
    region::AmiId,
    template::{TemplateName, TemplateSpec},
};
use kleya_core::test_support::InMemoryCompute;

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
