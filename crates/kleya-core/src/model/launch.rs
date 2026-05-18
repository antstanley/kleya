use crate::model::{
    instance::InstanceName,
    key::KeyName,
    market::{MarketKind, SpotType},
    tag::Tag,
    template::TemplateName,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub template: TemplateName,
    pub instance_name: InstanceName,
    pub instance_type_override: Option<String>,
    pub market_override: Option<MarketKind>,
    pub spot_type_override: Option<SpotType>,
    pub extra_tags: Vec<Tag>,
    pub key_name: KeyName,
}

#[derive(Debug, Clone)]
pub struct Deadline {
    pub timeout: Duration,
    pub poll_interval: Duration,
    pub cancel: Option<CancellationToken>,
}
