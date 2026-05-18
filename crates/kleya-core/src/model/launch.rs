use crate::model::{
    instance::InstanceName,
    key::KeyName,
    market::{MarketKind, SpotType},
    tag::Tag,
    template::TemplateName,
};
use std::time::Duration;

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

#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    pub timeout: Duration,
    pub poll_interval: Duration,
}
