use async_trait::async_trait;
use std::sync::Arc;

use aws_sdk_ec2::types as e;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;

use kleya_core::model::{
    instance::{Instance, InstanceFilter, InstanceId, InstanceState},
    key::{Fingerprint, KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    market::{MarketKind, SpotType},
    region::{AmiId, SecurityGroupId, SubnetId},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};
use kleya_core::ports::cloud_compute::CloudCompute;
use kleya_core::Result;

use crate::error::AwsError;

pub struct AwsEc2 {
    pub ec2: Arc<Ec2Client>,
    pub ssm: Arc<SsmClient>,
    pub region: String,
}

impl AwsEc2 {
    async fn authorize_default_ingress(&self, group_id: &str) -> Result<()> {
        let auth = self
            .ec2
            .authorize_security_group_ingress()
            .group_id(group_id)
            .ip_permissions(
                e::IpPermission::builder()
                    .ip_protocol("tcp")
                    .from_port(22)
                    .to_port(22)
                    .ip_ranges(e::IpRange::builder().cidr_ip("0.0.0.0/0").build())
                    .build(),
            )
            .send()
            .await;
        if let Err(err) = auth {
            if !has_code(&err, "InvalidPermission.Duplicate") {
                return Err(sdk(err).into());
            }
        }
        Ok(())
    }
}

fn sdk<E: std::error::Error + Send + Sync + 'static>(e: E) -> AwsError {
    AwsError::Sdk(Box::new(e))
}

use aws_sdk_ec2::error::ProvideErrorMetadata;

fn has_code<E, R>(err: &aws_sdk_ec2::error::SdkError<E, R>, code: &str) -> bool
where
    E: ProvideErrorMetadata,
{
    err.code() == Some(code)
}

fn build_request_launch_template_data(spec: &TemplateSpec) -> e::RequestLaunchTemplateData {
    let tags: Vec<e::Tag> = spec
        .tags
        .iter()
        .filter_map(|t| e::Tag::builder().key(&t.key).value(&t.value).build().into())
        .collect();
    let market = match spec.market {
        MarketKind::Spot => Some(
            e::LaunchTemplateInstanceMarketOptionsRequest::builder()
                .market_type(e::MarketType::Spot)
                .spot_options(
                    e::LaunchTemplateSpotMarketOptionsRequest::builder()
                        .spot_instance_type(match spec.spot_type {
                            SpotType::OneTime => e::SpotInstanceType::OneTime,
                            SpotType::Persistent => e::SpotInstanceType::Persistent,
                        })
                        .build(),
                )
                .build(),
        ),
        MarketKind::OnDemand => None,
    };
    let mut data = e::RequestLaunchTemplateData::builder()
        .instance_type(e::InstanceType::from(spec.instance_type.as_str()))
        .key_name(spec.key_name.as_str())
        .user_data(&spec.user_data_base64)
        .set_tag_specifications(Some(vec![
            e::LaunchTemplateTagSpecificationRequest::builder()
                .resource_type(e::ResourceType::Instance)
                .set_tags(Some(tags))
                .build(),
        ]));
    if !spec.security_group_ids.is_empty() {
        data = data.set_security_group_ids(Some(
            spec.security_group_ids
                .iter()
                .map(|s| s.0.clone())
                .collect(),
        ));
    }
    if let Some(a) = &spec.ami_id {
        data = data.image_id(a.0.clone());
    }
    if let Some(m) = market {
        data = data.instance_market_options(m);
    }
    data.build()
}

#[async_trait]
impl CloudCompute for AwsEc2 {
    async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId> {
        let data = build_request_launch_template_data(spec);
        let out = self
            .ec2
            .create_launch_template()
            .launch_template_name(&spec.name.0)
            .launch_template_data(data)
            .send()
            .await
            .map_err(sdk)?;
        let lt = out
            .launch_template()
            .ok_or(AwsError::MissingField("launch_template"))?;
        Ok(TemplateId(
            lt.launch_template_id().unwrap_or_default().to_string(),
        ))
    }

    async fn template_update(
        &self,
        id: &TemplateId,
        spec: &TemplateSpec,
    ) -> Result<TemplateVersion> {
        let data = build_request_launch_template_data(spec);
        let out = self
            .ec2
            .create_launch_template_version()
            .launch_template_id(&id.0)
            .launch_template_data(data)
            .send()
            .await
            .map_err(sdk)?;
        let ver = out
            .launch_template_version()
            .and_then(aws_sdk_ec2::types::LaunchTemplateVersion::version_number)
            .ok_or(AwsError::MissingField("version_number"))?;
        self.ec2
            .modify_launch_template()
            .launch_template_id(&id.0)
            .default_version(ver.to_string())
            .send()
            .await
            .map_err(sdk)?;
        Ok(TemplateVersion(u64::try_from(ver).unwrap_or(0)))
    }

    async fn template_list(&self) -> Result<Vec<TemplateSummary>> {
        let mut paginator = self.ec2.describe_launch_templates().into_paginator().send();
        let mut acc = Vec::new();
        while let Some(page) = paginator.next().await {
            let page = page.map_err(sdk)?;
            for lt in page.launch_templates() {
                if let (Some(id), Some(name)) = (lt.launch_template_id(), lt.launch_template_name())
                {
                    acc.push(TemplateSummary {
                        id: TemplateId(id.to_string()),
                        name: TemplateName(name.to_string()),
                        latest_version: TemplateVersion(
                            u64::try_from(lt.latest_version_number().unwrap_or(0)).unwrap_or(0),
                        ),
                    });
                }
            }
        }
        Ok(acc)
    }

    async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>> {
        let out = self
            .ec2
            .describe_launch_templates()
            .launch_template_names(&name.0)
            .send()
            .await;
        let Ok(out) = out else { return Ok(None) };
        Ok(out.launch_templates().first().and_then(|lt| {
            Some(TemplateSummary {
                id: TemplateId(lt.launch_template_id()?.to_string()),
                name: name.clone(),
                latest_version: TemplateVersion(
                    u64::try_from(lt.latest_version_number().unwrap_or(0)).unwrap_or(0),
                ),
            })
        }))
    }

    async fn template_delete(&self, id: &TemplateId) -> Result<()> {
        self.ec2
            .delete_launch_template()
            .launch_template_id(&id.0)
            .send()
            .await
            .map_err(sdk)?;
        Ok(())
    }

    async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance> {
        let tags = vec![
            e::Tag::builder()
                .key("Name")
                .value(req.instance_name.as_str())
                .build(),
            e::Tag::builder().key("kleya:managed").value("true").build(),
            e::Tag::builder()
                .key("kleya:template")
                .value(&req.template.0)
                .build(),
            e::Tag::builder()
                .key("kleya:key")
                .value(req.key_name.as_str())
                .build(),
        ];
        let mut run = self
            .ec2
            .run_instances()
            .launch_template(
                e::LaunchTemplateSpecification::builder()
                    .launch_template_name(&req.template.0)
                    .build(),
            )
            .min_count(1)
            .max_count(1)
            .tag_specifications(
                e::TagSpecification::builder()
                    .resource_type(e::ResourceType::Instance)
                    .set_tags(Some(tags))
                    .build(),
            );
        if let Some(t) = &req.instance_type_override {
            run = run.instance_type(e::InstanceType::from(t.as_str()));
        }
        if let Some(m) = req.market_override {
            let spot = matches!(m, MarketKind::Spot);
            if spot {
                let spot_type = match req.spot_type_override {
                    Some(SpotType::Persistent) => e::SpotInstanceType::Persistent,
                    _ => e::SpotInstanceType::OneTime,
                };
                run = run.instance_market_options(
                    e::InstanceMarketOptionsRequest::builder()
                        .market_type(e::MarketType::Spot)
                        .spot_options(
                            e::SpotMarketOptions::builder()
                                .spot_instance_type(spot_type)
                                .build(),
                        )
                        .build(),
                );
            }
        }
        let out = run.send().await.map_err(sdk)?;
        let inst = out
            .instances()
            .first()
            .ok_or(AwsError::MissingField("instances[0]"))?;
        crate::mapping::map_instance(inst)
            .ok_or_else(|| AwsError::MissingField("instance fields").into())
    }

    async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>> {
        let mut req = self.ec2.describe_instances();
        if filter.managed_only {
            req = req.filters(
                e::Filter::builder()
                    .name("tag:kleya:managed")
                    .values("true")
                    .build(),
            );
        }
        if let Some(n) = &filter.name {
            req = req.filters(e::Filter::builder().name("tag:Name").values(n).build());
        }
        let out = req.send().await.map_err(sdk)?;
        let mut acc = vec![];
        for r in out.reservations() {
            for i in r.instances() {
                if let Some(inst) = crate::mapping::map_instance(i) {
                    acc.push(inst);
                }
            }
        }
        Ok(acc)
    }

    async fn instance_describe(&self, id: &InstanceId) -> Result<Instance> {
        let out = self
            .ec2
            .describe_instances()
            .instance_ids(id.as_str())
            .send()
            .await
            .map_err(sdk)?;
        let inst = out
            .reservations()
            .first()
            .and_then(|r| r.instances().first())
            .ok_or_else(|| kleya_core::Error::InstanceNotFound {
                name: id.as_str().into(),
                region: self.region.clone(),
            })?;
        crate::mapping::map_instance(inst)
            .ok_or_else(|| AwsError::MissingField("instance fields").into())
    }

    async fn instance_terminate(&self, id: &InstanceId) -> Result<()> {
        self.ec2
            .terminate_instances()
            .instance_ids(id.as_str())
            .send()
            .await
            .map_err(sdk)?;
        Ok(())
    }

    async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance> {
        assert!(deadline.poll_interval.as_secs() > 0, "poll_interval is 0");
        assert!(deadline.timeout > deadline.poll_interval, "timeout < poll");
        let start = std::time::Instant::now();
        let max_attempts: u32 =
            u32::try_from(deadline.timeout.as_secs() / deadline.poll_interval.as_secs() + 2)
                .unwrap_or(u32::MAX);
        let mut attempts: u32 = 0;
        loop {
            attempts = attempts.saturating_add(1);
            let inst = self.instance_describe(id).await?;
            if matches!(inst.state, InstanceState::Running) {
                return Ok(inst);
            }
            if start.elapsed() >= deadline.timeout || attempts >= max_attempts {
                return Err(kleya_core::Error::LaunchWaitTimeout {
                    instance: id.clone(),
                    elapsed_seconds: u32::try_from(start.elapsed().as_secs()).unwrap_or(u32::MAX),
                });
            }
            if kleya_core::util::wait_or_cancel(deadline.poll_interval, deadline.cancel.as_ref())
                .await
            {
                return Err(kleya_core::Error::Cancelled {
                    instance: id.clone(),
                });
            }
        }
    }

    async fn ensure_default_template(&self, spec: &TemplateSpec) -> Result<TemplateId> {
        if let Some(existing) = self.template_get_by_name(&spec.name).await? {
            return Ok(existing.id);
        }
        self.template_create(spec).await
    }

    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId> {
        let mut existing: Option<String> = None;
        if let Ok(out) = self
            .ec2
            .describe_security_groups()
            .group_names(name)
            .send()
            .await
        {
            if let Some(g) = out.security_groups().first() {
                if let Some(id) = g.group_id() {
                    existing = Some(id.to_string());
                }
            }
        }
        if let Some(id) = existing {
            self.authorize_default_ingress(&id).await?;
            return Ok(SecurityGroupId(id));
        }
        let created = self
            .ec2
            .create_security_group()
            .group_name(name)
            .description("kleya managed default SG")
            .send()
            .await;
        let id = match created {
            Ok(out) => out
                .group_id()
                .ok_or(AwsError::MissingField("group_id"))?
                .to_string(),
            Err(err) => {
                if has_code(&err, "InvalidGroup.Duplicate") {
                    let again = self
                        .ec2
                        .describe_security_groups()
                        .group_names(name)
                        .send()
                        .await
                        .map_err(sdk)?;
                    again
                        .security_groups()
                        .first()
                        .and_then(|g| g.group_id())
                        .ok_or(AwsError::MissingField("group_id"))?
                        .to_string()
                } else {
                    return Err(sdk(err).into());
                }
            }
        };
        self.authorize_default_ingress(&id).await?;
        Ok(SecurityGroupId(id))
    }

    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()> {
        if let Ok(out) = self
            .ec2
            .describe_key_pairs()
            .key_names(name.as_str())
            .send()
            .await
        {
            if !out.key_pairs().is_empty() {
                return Ok(());
            }
        }
        let res = self
            .ec2
            .import_key_pair()
            .key_name(name.as_str())
            .public_key_material(aws_sdk_ec2::primitives::Blob::new(public_key.0.as_bytes()))
            .send()
            .await;
        if let Err(err) = res {
            if !has_code(&err, "InvalidKeyPair.Duplicate") {
                return Err(sdk(err).into());
            }
        }
        let confirmed = self
            .ec2
            .describe_key_pairs()
            .key_names(name.as_str())
            .send()
            .await
            .map_err(sdk)?;
        if confirmed.key_pairs().is_empty() {
            return Err(AwsError::MissingField("key_pair after ensure").into());
        }
        Ok(())
    }

    async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<Fingerprint>> {
        let out = self
            .ec2
            .describe_key_pairs()
            .key_names(name.as_str())
            .send()
            .await;
        let Ok(out) = out else { return Ok(None) };
        Ok(out
            .key_pairs()
            .first()
            .and_then(|k| k.key_fingerprint())
            .map(|s| Fingerprint(s.to_string())))
    }

    async fn resolve_default_subnet(&self) -> Result<SubnetId> {
        let vpcs = self
            .ec2
            .describe_vpcs()
            .filters(
                e::Filter::builder()
                    .name("isDefault")
                    .values("true")
                    .build(),
            )
            .send()
            .await
            .map_err(sdk)?;
        let vpc_id = vpcs
            .vpcs()
            .first()
            .and_then(|v| v.vpc_id())
            .ok_or_else(|| kleya_core::Error::ConfigInvalid {
                reason: format!("no default VPC in region {}", self.region),
            })?;
        let subs = self
            .ec2
            .describe_subnets()
            .filters(e::Filter::builder().name("vpc-id").values(vpc_id).build())
            .send()
            .await
            .map_err(sdk)?;
        let mut picked: Option<&e::Subnet> = None;
        for s in subs.subnets() {
            match (picked, s.availability_zone()) {
                (None, Some(_)) => picked = Some(s),
                (Some(cur), Some(az)) if az < cur.availability_zone().unwrap_or("") => {
                    picked = Some(s);
                }
                _ => {}
            }
        }
        let id =
            picked
                .and_then(|s| s.subnet_id())
                .ok_or_else(|| kleya_core::Error::ConfigInvalid {
                    reason: format!("no subnet in default VPC of region {}", self.region),
                })?;
        Ok(SubnetId(id.to_string()))
    }

    async fn resolve_ami_alias(&self, alias: &str) -> Result<AmiId> {
        let param = match alias {
            "amazon-linux-2023-arm64" => {
                "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64"
            }
            "amazon-linux-2023-x86_64" => {
                "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64"
            }
            other => {
                return Err(kleya_core::Error::ConfigInvalid {
                    reason: format!("unknown ami_alias '{other}'"),
                });
            }
        };
        let out = self
            .ssm
            .get_parameter()
            .name(param)
            .send()
            .await
            .map_err(sdk)?;
        let val = out
            .parameter()
            .and_then(|p| p.value())
            .ok_or_else(|| AwsError::SsmMissing(param.into()))?;
        Ok(AmiId(val.to_string()))
    }
}
