#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::items_after_statements
)]

#[path = "mod.rs"]
mod harness;
use harness::*;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires KLEYA_TEST_FLOCI=1 and a running floci endpoint"]
async fn create_list_delete_template() {
    let Some(endpoint) = ensure_floci() else {
        return;
    };
    let ec2 = Arc::new(ec2(&endpoint).await);
    let ssm = Arc::new({
        use aws_config::BehaviorVersion;
        let cfg = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_sdk_ec2::config::Region::new("eu-west-1"))
            .load()
            .await;
        aws_sdk_ssm::Client::new(&cfg)
    });
    let adapter = kleya_aws::ec2::AwsEc2 {
        ec2,
        ssm,
        region: "eu-west-1".into(),
    };

    use kleya_core::model;
    let spec = kleya_core::model::template::TemplateSpec {
        name: model::template::TemplateName("floci-t1".into()),
        ami_id: Some(model::region::AmiId("ami-00000000000000001".into())),
        ami_alias: None,
        instance_type: "t3.micro".into(),
        key_name: model::key::KeyName::new("kleya-default").unwrap(),
        security_group_ids: vec![],
        subnet_id: None,
        market: model::market::MarketKind::Spot,
        spot_type: model::market::SpotType::OneTime,
        tags: vec![],
        user_data_base64: "H4sIAAAA".into(),
    };
    use kleya_core::ports::cloud_compute::CloudCompute;
    let id = adapter.template_create(&spec).await.expect("create");
    let listed = adapter.template_list().await.expect("list");
    assert!(listed.iter().any(|t| t.id == id));
    adapter.template_delete(&id).await.expect("delete");
}
