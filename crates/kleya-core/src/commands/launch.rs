use std::sync::Arc;
use std::time::Duration;

use crate::bootstrap::{
    encode::encode_user_data,
    render::{render, BootstrapVars},
};
use crate::config::Config;
use crate::error::Error;
use crate::limits::{LAUNCH_POLL_INTERVAL_SECONDS, LAUNCH_WAIT_SECONDS_MAX};
use crate::model::{
    instance::{Instance, InstanceName},
    key::KeyName,
    launch::{Deadline, LaunchRequest},
    market::{MarketKind, SpotType},
    region::AmiId,
    tag::Tag,
    template::{TemplateName, TemplateSpec},
};
use crate::ports::{cloud_compute::CloudCompute, id_gen::IdGen, key_store::KeyStore};
use crate::Result;

pub struct LaunchService {
    pub compute: Arc<dyn CloudCompute>,
    pub key_store: Arc<dyn KeyStore>,
    pub id_gen: Arc<dyn IdGen>,
    pub config: Arc<Config>,
    pub bootstrap_tpl: &'static str,
    pub ghostty_tinfo: &'static str,
}

pub struct LaunchOpts {
    pub template_name: Option<String>,
    pub instance_name: Option<String>,
    pub dry_run: bool,
}

pub struct LaunchPlan {
    pub template: TemplateName,
    pub instance_name: InstanceName,
    pub key_name: KeyName,
    pub ami_id: AmiId,
}

impl LaunchService {
    pub async fn run(&self, opts: LaunchOpts) -> Result<Option<Instance>> {
        let plan = self.build_plan(&opts).await?;
        if opts.dry_run {
            tracing::info!(
                template = %plan.template.0,
                instance = %plan.instance_name.as_str(),
                key = %plan.key_name.as_str(),
                ami = %plan.ami_id.0,
                "dry-run plan"
            );
            return Ok(None);
        }
        self.ensure_template(&plan).await?;
        let inst = self
            .compute
            .instance_launch(&LaunchRequest {
                template: plan.template.clone(),
                instance_name: plan.instance_name.clone(),
                instance_type_override: None,
                market_override: None,
                spot_type_override: None,
                extra_tags: vec![],
                key_name: plan.key_name.clone(),
            })
            .await?;
        let deadline = Deadline {
            timeout: Duration::from_secs(u64::from(LAUNCH_WAIT_SECONDS_MAX)),
            poll_interval: Duration::from_secs(u64::from(LAUNCH_POLL_INTERVAL_SECONDS)),
        };
        let running = self
            .compute
            .instance_wait_running(&inst.id, deadline)
            .await?;
        Ok(Some(running))
    }

    async fn build_plan(&self, opts: &LaunchOpts) -> Result<LaunchPlan> {
        let template_name = TemplateName(
            opts.template_name
                .clone()
                .unwrap_or_else(|| "default".into()),
        );
        let instance_name = match &opts.instance_name {
            Some(n) => InstanceName::new(n)?,
            None => InstanceName::new(self.id_gen.name())?,
        };
        let key_name = KeyName::new(self.config.keys.default_key_name.clone())?;
        let ami_id = self
            .compute
            .resolve_ami_alias(&self.config.defaults.ami_alias)
            .await?;
        Ok(LaunchPlan {
            template: template_name,
            instance_name,
            key_name,
            ami_id,
        })
    }

    async fn ensure_template(&self, plan: &LaunchPlan) -> Result<()> {
        if self
            .compute
            .template_get_by_name(&plan.template)
            .await?
            .is_some()
        {
            self.assert_key_synced(&plan.key_name).await?;
            return Ok(());
        }
        let subnet = self.compute.resolve_default_subnet().await?;
        let sg = self
            .compute
            .ensure_default_security_group("kleya-default")
            .await?;
        self.ensure_keypair(&plan.key_name).await?;
        let vars = BootstrapVars::default_with(self.ghostty_tinfo);
        let rendered = render(self.bootstrap_tpl, &vars)?;
        let user_data_b64 = encode_user_data(&rendered)?;
        let spec = TemplateSpec {
            name: plan.template.clone(),
            ami_id: Some(plan.ami_id.clone()),
            ami_alias: None,
            instance_type: self.config.defaults.instance_type.clone(),
            key_name: plan.key_name.clone(),
            security_group_ids: vec![sg],
            subnet_id: Some(subnet),
            market: match self.config.defaults.market.as_str() {
                "on-demand" => MarketKind::OnDemand,
                _ => MarketKind::Spot,
            },
            spot_type: match self.config.defaults.spot_type.as_str() {
                "persistent" => SpotType::Persistent,
                _ => SpotType::OneTime,
            },
            tags: vec![Tag::new("Project", "kleya")?],
            user_data_base64: user_data_b64,
        };
        self.compute.template_create(&spec).await?;
        Ok(())
    }

    async fn ensure_keypair(&self, name: &KeyName) -> Result<()> {
        match (
            self.key_store.exists(name),
            self.compute.keypair_fingerprint(name).await?,
        ) {
            (true, Some(cloud_fp)) => {
                let local_fp = self.key_store.fingerprint(name)?;
                if local_fp != cloud_fp {
                    return Err(Error::KeyMismatch {
                        name: name.to_string(),
                    });
                }
                Ok(())
            }
            (true, None) => {
                let public = self.key_store.read_public(name)?;
                self.compute.ensure_default_keypair(name, &public).await
            }
            (false, Some(_)) => Err(Error::KeyOrphaned {
                name: name.to_string(),
            }),
            (false, None) => {
                let pair = self.key_store.generate(name)?;
                self.compute
                    .ensure_default_keypair(name, &pair.public)
                    .await
            }
        }
    }

    #[allow(clippy::unused_async)]
    async fn assert_key_synced(&self, name: &KeyName) -> Result<()> {
        if !self.key_store.exists(name) {
            return Err(Error::KeyOrphaned {
                name: name.to_string(),
            });
        }
        Ok(())
    }
}
