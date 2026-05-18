use async_trait::async_trait;
use std::sync::Arc;

use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;

use kleya_core::model::{
    instance::{Instance, InstanceFilter, InstanceId},
    key::{Fingerprint, KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    region::{AmiId, SecurityGroupId, SubnetId},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};
use kleya_core::ports::cloud_compute::CloudCompute;
use kleya_core::Result;

pub struct AwsEc2 {
    pub ec2: Arc<Ec2Client>,
    pub ssm: Arc<SsmClient>,
    pub region: String,
}

impl AwsEc2 {
    fn unsupported(&self, what: &str) -> kleya_core::Error {
        let _ = self;
        kleya_core::Error::ConfigInvalid {
            reason: format!("{what} not supported in v1"),
        }
    }
}

#[async_trait]
impl CloudCompute for AwsEc2 {
    async fn template_create(&self, _spec: &TemplateSpec) -> Result<TemplateId> {
        Err(self.unsupported("template_create (Task 16)"))
    }
    async fn template_update(
        &self,
        _id: &TemplateId,
        _spec: &TemplateSpec,
    ) -> Result<TemplateVersion> {
        Err(self.unsupported("template_update (Task 16)"))
    }
    async fn template_list(&self) -> Result<Vec<TemplateSummary>> {
        Ok(vec![])
    }
    async fn template_delete(&self, _id: &TemplateId) -> Result<()> {
        Ok(())
    }
    async fn template_get_by_name(&self, _name: &TemplateName) -> Result<Option<TemplateSummary>> {
        Ok(None)
    }
    async fn instance_launch(&self, _req: &LaunchRequest) -> Result<Instance> {
        Err(self.unsupported("instance_launch (Task 16)"))
    }
    async fn instance_list(&self, _filter: &InstanceFilter) -> Result<Vec<Instance>> {
        Ok(vec![])
    }
    async fn instance_describe(&self, _id: &InstanceId) -> Result<Instance> {
        Err(self.unsupported("instance_describe (Task 16)"))
    }
    async fn instance_terminate(&self, _id: &InstanceId) -> Result<()> {
        Ok(())
    }
    async fn instance_wait_running(&self, _id: &InstanceId, _d: Deadline) -> Result<Instance> {
        Err(self.unsupported("instance_wait_running (Task 16)"))
    }
    async fn ensure_default_security_group(&self, _name: &str) -> Result<SecurityGroupId> {
        Err(self.unsupported("ensure_default_security_group (Task 16)"))
    }
    async fn ensure_default_keypair(&self, _name: &KeyName, _public_key: &PublicKey) -> Result<()> {
        Ok(())
    }
    async fn keypair_fingerprint(&self, _name: &KeyName) -> Result<Option<Fingerprint>> {
        Ok(None)
    }
    async fn resolve_default_subnet(&self) -> Result<SubnetId> {
        Err(self.unsupported("resolve_default_subnet (Task 16)"))
    }
    async fn resolve_ami_alias(&self, _alias: &str) -> Result<AmiId> {
        Err(self.unsupported("resolve_ami_alias (Task 16)"))
    }
}
