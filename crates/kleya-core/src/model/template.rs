use crate::model::{
    key::KeyName,
    market::{MarketKind, SpotType},
    region::{AmiId, SecurityGroupId, SubnetId},
    tag::Tag,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateName(pub String);

#[derive(Debug, Clone)]
pub struct TemplateSpec {
    pub name: TemplateName,
    pub ami_id: Option<AmiId>,
    pub ami_alias: Option<String>,
    pub instance_type: String,
    pub key_name: KeyName,
    pub security_group_ids: Vec<SecurityGroupId>,
    pub subnet_id: Option<SubnetId>,
    pub market: MarketKind,
    pub spot_type: SpotType,
    pub tags: Vec<Tag>,
    pub user_data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVersion(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: TemplateId,
    pub name: TemplateName,
    pub latest_version: TemplateVersion,
}
