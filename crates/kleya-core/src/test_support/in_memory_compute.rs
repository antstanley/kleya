use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::Error;
use crate::model::{
    instance::{Instance, InstanceFilter, InstanceId, InstanceName, InstanceState},
    key::{Fingerprint, KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    region::{AmiId, SecurityGroupId, SubnetId},
    tag::{Tag, KLEYA_TAG_KEY, KLEYA_TAG_MANAGED, KLEYA_TAG_NAME, KLEYA_TAG_TEMPLATE},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};
use crate::ports::cloud_compute::CloudCompute;
use crate::Result;

#[derive(Default)]
struct State {
    templates: HashMap<TemplateName, (TemplateId, TemplateSpec, TemplateVersion)>,
    instances: HashMap<InstanceId, Instance>,
    sgs: HashMap<String, SecurityGroupId>,
    keypairs: HashMap<KeyName, String>,
    next_id: u64,
}

pub struct InMemoryCompute {
    state: Mutex<State>,
    default_subnet: SubnetId,
    default_ami: AmiId,
}

impl InMemoryCompute {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            default_subnet: SubnetId("subnet-fake".into()),
            default_ami: AmiId("ami-fake".into()),
        }
    }

    fn next_instance_id(&self) -> InstanceId {
        let mut s = self.state.lock().expect("mutex");
        s.next_id += 1;
        InstanceId::new(format!("i-{:016x}", s.next_id)).expect("ids valid")
    }
}

impl Default for InMemoryCompute {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CloudCompute for InMemoryCompute {
    async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId> {
        let mut s = self.state.lock().expect("mutex");
        let id = TemplateId(format!("lt-{}", s.templates.len()));
        s.templates.insert(
            spec.name.clone(),
            (id.clone(), spec.clone(), TemplateVersion(1)),
        );
        Ok(id)
    }

    async fn template_update(
        &self,
        id: &TemplateId,
        spec: &TemplateSpec,
    ) -> Result<TemplateVersion> {
        let mut s = self.state.lock().expect("mutex");
        let entry = s
            .templates
            .values_mut()
            .find(|(tid, _, _)| tid == id)
            .ok_or_else(|| Error::ConfigInvalid {
                reason: format!("template not found: {}", id.0),
            })?;
        entry.1 = spec.clone();
        entry.2 = TemplateVersion(entry.2 .0 + 1);
        Ok(entry.2.clone())
    }

    async fn template_list(&self) -> Result<Vec<TemplateSummary>> {
        let s = self.state.lock().expect("mutex");
        Ok(s.templates
            .iter()
            .map(|(name, (id, _, ver))| TemplateSummary {
                id: id.clone(),
                name: name.clone(),
                latest_version: ver.clone(),
            })
            .collect())
    }

    async fn template_delete(&self, id: &TemplateId) -> Result<()> {
        let mut s = self.state.lock().expect("mutex");
        s.templates.retain(|_, (tid, _, _)| tid != id);
        Ok(())
    }

    async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>> {
        let s = self.state.lock().expect("mutex");
        Ok(s.templates.get(name).map(|(id, _, ver)| TemplateSummary {
            id: id.clone(),
            name: name.clone(),
            latest_version: ver.clone(),
        }))
    }

    async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance> {
        let id = self.next_instance_id();
        let tags = vec![
            Tag::new(KLEYA_TAG_NAME, req.instance_name.as_str())?,
            Tag::new(KLEYA_TAG_MANAGED, "true")?,
            Tag::new(KLEYA_TAG_TEMPLATE, &req.template.0)?,
            Tag::new(KLEYA_TAG_KEY, req.key_name.as_str())?,
        ];
        let inst = Instance {
            id: id.clone(),
            name: Some(req.instance_name.clone()),
            state: InstanceState::Pending,
            public_dns: Some(format!("{}.example", id.as_str())),
            public_ip: Some("203.0.113.10".into()),
            tags,
        };
        self.state
            .lock()
            .expect("mutex")
            .instances
            .insert(id.clone(), inst.clone());
        Ok(inst)
    }

    async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>> {
        let s = self.state.lock().expect("mutex");
        let out = s
            .instances
            .values()
            .filter(|i| {
                if filter.managed_only
                    && !i
                        .tags
                        .iter()
                        .any(|t| t.key == KLEYA_TAG_MANAGED && t.value == "true")
                {
                    return false;
                }
                if let Some(n) = &filter.name {
                    if i.name.as_ref().map(InstanceName::as_str) != Some(n.as_str()) {
                        return false;
                    }
                }
                if !filter.states.is_empty() && !filter.states.contains(&i.state) {
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        Ok(out)
    }

    async fn instance_describe(&self, id: &InstanceId) -> Result<Instance> {
        self.state
            .lock()
            .expect("mutex")
            .instances
            .get(id)
            .cloned()
            .ok_or_else(|| Error::InstanceNotFound {
                name: id.as_str().into(),
                region: "fake".into(),
            })
    }

    async fn instance_terminate(&self, id: &InstanceId) -> Result<()> {
        let mut s = self.state.lock().expect("mutex");
        if let Some(i) = s.instances.get_mut(id) {
            i.state = InstanceState::Terminated;
        }
        Ok(())
    }

    async fn instance_wait_running(
        &self,
        id: &InstanceId,
        _deadline: Deadline,
    ) -> Result<Instance> {
        let mut s = self.state.lock().expect("mutex");
        let i = s
            .instances
            .get_mut(id)
            .ok_or_else(|| Error::InstanceNotFound {
                name: id.as_str().into(),
                region: "fake".into(),
            })?;
        i.state = InstanceState::Running;
        Ok(i.clone())
    }

    async fn ensure_default_template(&self, spec: &TemplateSpec) -> Result<TemplateId> {
        {
            let s = self.state.lock().expect("mutex");
            if let Some((id, _, _)) = s.templates.get(&spec.name) {
                return Ok(id.clone());
            }
        }
        self.template_create(spec).await
    }

    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId> {
        let mut s = self.state.lock().expect("mutex");
        let next = s.sgs.len() + 1;
        let id = s
            .sgs
            .entry(name.to_string())
            .or_insert(SecurityGroupId(format!("sg-{next}")))
            .clone();
        Ok(id)
    }

    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()> {
        let mut s = self.state.lock().expect("mutex");
        s.keypairs.entry(name.clone()).or_insert_with(|| {
            format!("fake-md5:{:08x}", crc32fast::hash(public_key.0.as_bytes()))
        });
        Ok(())
    }

    async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<Fingerprint>> {
        Ok(self
            .state
            .lock()
            .expect("mutex")
            .keypairs
            .get(name)
            .cloned()
            .map(Fingerprint))
    }

    async fn keypair_delete(&self, name: &KeyName) -> Result<()> {
        self.state.lock().expect("mutex").keypairs.remove(name);
        Ok(())
    }

    async fn resolve_default_subnet(&self) -> Result<SubnetId> {
        Ok(self.default_subnet.clone())
    }
    async fn resolve_ami_alias(&self, _alias: &str) -> Result<AmiId> {
        Ok(self.default_ami.clone())
    }
}
