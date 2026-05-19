use aws_sdk_ec2::types as e;
use kleya_core::model::{
    instance::{Instance, InstanceId, InstanceName, InstanceState},
    tag::{Tag, KLEYA_TAG_NAME},
};

#[must_use]
pub fn map_instance(i: &e::Instance) -> Option<Instance> {
    let id = InstanceId::new(i.instance_id()?).ok()?;
    let state = match i.state().and_then(|s| s.name()) {
        Some(e::InstanceStateName::Pending) => InstanceState::Pending,
        Some(e::InstanceStateName::Running) => InstanceState::Running,
        Some(e::InstanceStateName::ShuttingDown) => InstanceState::ShuttingDown,
        Some(e::InstanceStateName::Stopped) => InstanceState::Stopped,
        Some(e::InstanceStateName::Stopping) => InstanceState::Stopping,
        Some(e::InstanceStateName::Terminated) => InstanceState::Terminated,
        Some(other) => InstanceState::Other(other.as_str().into()),
        None => InstanceState::Other("unknown".into()),
    };
    let tags: Vec<Tag> = i
        .tags()
        .iter()
        .filter_map(|t| {
            let key = t.key()?;
            let value = t.value()?;
            match Tag::new(key, value) {
                Ok(tag) => Some(tag),
                Err(err) => {
                    tracing::warn!(
                        key = %key,
                        error = %err,
                        "dropping instance tag that fails kleya validation",
                    );
                    None
                }
            }
        })
        .collect();
    let name = tags
        .iter()
        .find(|t| t.key() == KLEYA_TAG_NAME)
        .and_then(|t| InstanceName::new(t.value()).ok());
    Some(Instance {
        id,
        name,
        state,
        public_dns: i
            .public_dns_name()
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        public_ip: i.public_ip_address().map(str::to_string),
        tags,
    })
}
