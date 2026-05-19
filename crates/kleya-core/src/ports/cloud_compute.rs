use crate::model::{
    instance::{Instance, InstanceFilter, InstanceId},
    key::{Fingerprint, KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    region::{AmiId, SecurityGroupId, SubnetId},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};
use crate::Result;
use async_trait::async_trait;

#[async_trait]
pub trait CloudCompute: Send + Sync {
    async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId>;
    async fn template_update(
        &self,
        id: &TemplateId,
        spec: &TemplateSpec,
    ) -> Result<TemplateVersion>;
    async fn template_list(&self) -> Result<Vec<TemplateSummary>>;
    async fn template_delete(&self, id: &TemplateId) -> Result<()>;
    async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>>;

    async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance>;
    async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>>;
    async fn instance_describe(&self, id: &InstanceId) -> Result<Instance>;
    async fn instance_terminate(&self, id: &InstanceId) -> Result<()>;
    async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance>;

    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId>;
    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()>;
    async fn ensure_default_template(&self, spec: &TemplateSpec) -> Result<TemplateId>;
    async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<Fingerprint>>;
    /// Identifies the fingerprint format returned by `keypair_fingerprint`.
    /// Adapters must return a stable, lowercase, kebab-case label. AWS EC2
    /// uses `"md5-spki-ed25519"` (MD5 of DER-encoded `SubjectPublicKeyInfo`
    /// of the Ed25519 public key — what `DescribeKeyPairs` returns for
    /// `ImportKeyPair`-imported keys).
    fn fingerprint_algorithm(&self) -> &'static str;
    /// Delete a registered keypair by name. Idempotent: a missing key is
    /// treated as success. Adapters must confirm absence afterwards.
    async fn keypair_delete(&self, name: &KeyName) -> Result<()>;
    async fn resolve_default_subnet(&self) -> Result<SubnetId>;
    async fn resolve_ami_alias(&self, alias: &str) -> Result<AmiId>;
}
