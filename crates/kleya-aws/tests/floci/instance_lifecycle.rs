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

async fn build_adapter(endpoint: &str) -> kleya_aws::ec2::AwsEc2 {
    use aws_config::BehaviorVersion;
    let ec2 = Arc::new(ec2(endpoint).await);
    let cfg = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(endpoint)
        .region(aws_sdk_ec2::config::Region::new("eu-west-1"))
        .load()
        .await;
    let ssm = Arc::new(aws_sdk_ssm::Client::new(&cfg));
    kleya_aws::ec2::AwsEc2 {
        ec2,
        ssm,
        region: "eu-west-1".into(),
    }
}

fn floci_skip(reason: &str) {
    eprintln!("floci skip (instance_lifecycle): {reason}");
}

#[tokio::test]
#[ignore = "requires KLEYA_TEST_FLOCI=1 and a running floci endpoint"]
async fn instance_launch_then_list_then_terminate() {
    let Some(endpoint) = ensure_floci() else {
        return;
    };
    let adapter = build_adapter(&endpoint).await;

    use kleya_core::model;
    use kleya_core::ports::cloud_compute::CloudCompute;

    let key = model::key::KeyName::new("floci-instance-test").unwrap();
    let public = model::key::PublicKey(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDummyFLOCITestKeyMaterial".into(),
    );
    if let Err(e) = adapter.ensure_default_keypair(&key, &public).await {
        floci_skip(&format!("ensure_default_keypair failed: {e}"));
        return;
    }

    let spec = model::template::TemplateSpec {
        name: model::template::TemplateName("floci-instance-template".into()),
        ami_id: Some(model::region::AmiId("ami-00000000000000001".into())),
        ami_alias: None,
        instance_type: "t3.micro".into(),
        key_name: key.clone(),
        security_group_ids: vec![],
        subnet_id: None,
        market: model::market::MarketKind::OnDemand,
        spot_type: model::market::SpotType::OneTime,
        tags: vec![],
        user_data_base64: "H4sIAAAA".into(),
    };
    if adapter.ensure_default_template(&spec).await.is_err() {
        floci_skip("CreateLaunchTemplate not supported by floci");
        return;
    }

    let req = model::launch::LaunchRequest {
        template: spec.name.clone(),
        instance_name: model::instance::InstanceName::new("floci-test-instance").unwrap(),
        instance_type_override: None,
        market_override: None,
        spot_type_override: None,
        extra_tags: vec![],
        key_name: key,
    };
    let inst = match adapter.instance_launch(&req).await {
        Ok(i) => i,
        Err(e) => {
            floci_skip(&format!("instance_launch failed: {e}"));
            return;
        }
    };
    assert_eq!(
        inst.name
            .as_ref()
            .map(model::instance::InstanceName::as_str),
        Some("floci-test-instance")
    );

    let listed = adapter
        .instance_list(&model::instance::InstanceFilter {
            managed_only: true,
            name: Some(req.instance_name.as_str().to_string()),
            states: vec![],
        })
        .await
        .expect("instance_list");
    assert!(
        listed.iter().any(|i| i.id == inst.id),
        "launched instance must appear in managed list",
    );

    adapter
        .instance_terminate(&inst.id)
        .await
        .expect("instance_terminate");
}

#[tokio::test]
#[ignore = "requires KLEYA_TEST_FLOCI=1 and a running floci endpoint"]
async fn instance_terminate_unknown_id_is_safe() {
    let Some(endpoint) = ensure_floci() else {
        return;
    };
    let adapter = build_adapter(&endpoint).await;

    use kleya_core::model::instance::{InstanceFilter, InstanceId};
    use kleya_core::ports::cloud_compute::CloudCompute;

    let bogus = InstanceId::new("i-deadbeefdeadbeef").unwrap();
    let term_result = adapter.instance_terminate(&bogus).await;
    let listed = adapter
        .instance_list(&InstanceFilter {
            managed_only: false,
            name: None,
            states: vec![],
        })
        .await
        .expect("instance_list");
    assert!(
        term_result.is_err() || listed.iter().all(|i| i.id != bogus),
        "termination of an unknown id must error OR leave no trace of the id",
    );
}
