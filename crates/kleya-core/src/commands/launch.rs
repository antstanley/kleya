use std::sync::Arc;
use std::time::Duration;

use crate::bootstrap::{
    encode::{encode_user_data, encode_user_data_passthrough},
    render::{render, BootstrapVars},
};

fn shellexpand_tilde(p: &str) -> std::path::PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(p)
}
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
}

pub struct LaunchOpts {
    pub template_name: Option<String>,
    pub instance_name: Option<String>,
    pub instance_type: Option<String>,
    pub market: Option<MarketKind>,
    pub dry_run: bool,
    /// On (local-absent, EC2-present): delete the EC2 key, regenerate locally,
    /// re-register. Without this flag the case is fatal `Error::KeyOrphaned`.
    pub regenerate_key: bool,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
}

pub struct LaunchPlan {
    pub template: TemplateName,
    pub instance_name: InstanceName,
    pub key_name: KeyName,
    pub ami_id: AmiId,
    pub regenerate_key: bool,
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
                instance_type_override: opts.instance_type.clone(),
                market_override: opts.market,
                spot_type_override: None,
                extra_tags: vec![],
                key_name: plan.key_name.clone(),
            })
            .await?;
        let deadline = Deadline {
            timeout: Duration::from_secs(u64::from(LAUNCH_WAIT_SECONDS_MAX)),
            poll_interval: Duration::from_secs(u64::from(LAUNCH_POLL_INTERVAL_SECONDS)),
            cancel: opts.cancel.clone(),
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
            regenerate_key: opts.regenerate_key,
        })
    }

    async fn ensure_template(&self, plan: &LaunchPlan) -> Result<()> {
        self.ensure_keypair(&plan.key_name, plan.regenerate_key)
            .await?;
        if self
            .compute
            .template_get_by_name(&plan.template)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let subnet = self.compute.resolve_default_subnet().await?;
        let sg = self
            .compute
            .ensure_default_security_group("kleya-default")
            .await?;
        let user_data_b64 = self.render_user_data().await?;
        let mut tags = vec![Tag::new("Project", "kleya")?];
        if let Some(t) = self
            .config
            .templates
            .iter()
            .find(|t| t.name == plan.template.0)
        {
            for tag in &t.tags {
                tags.push(Tag::new(&tag.key, &tag.value)?);
            }
        }
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
            tags,
            user_data_base64: user_data_b64,
        };
        self.compute.ensure_default_template(&spec).await?;
        Ok(())
    }

    async fn render_user_data(&self) -> Result<String> {
        if let Some(path) = &self.config.bootstrap.user_data_path {
            if self.config.bootstrap.install_ghostty_terminfo {
                tracing::warn!(
                    "bootstrap.user_data_path is set; install_ghostty_terminfo has no effect"
                );
            }
            let expanded = shellexpand_tilde(path);
            let bytes = tokio::fs::read(&expanded).await?;
            let raw = String::from_utf8(bytes).map_err(|e| Error::ConfigInvalid {
                reason: format!("user-data override not utf-8: {e}"),
            })?;
            return encode_user_data_passthrough(&raw);
        }
        let vars = BootstrapVars::default_with(kleya_bootstrap_assets::GHOSTTY_TERMINFO);
        let rendered = render(&vars)?;
        encode_user_data(&rendered)
    }

    async fn ensure_keypair(&self, name: &KeyName, regenerate: bool) -> Result<()> {
        match (
            self.key_store.exists(name),
            self.compute.keypair_fingerprint(name).await?,
        ) {
            (true, Some(cloud_fp)) => {
                let local_fp = self.key_store.fingerprint(name)?;
                if local_fp != cloud_fp {
                    return Err(Error::KeyMismatch { name: name.clone() });
                }
                Ok(())
            }
            (true, None) => {
                let public = self.key_store.read_public(name)?;
                self.compute.ensure_default_keypair(name, &public).await
            }
            (false, Some(_)) if regenerate => {
                self.compute.keypair_delete(name).await?;
                let pair = self.key_store.generate(name)?;
                self.compute
                    .ensure_default_keypair(name, &pair.public)
                    .await
            }
            (false, Some(_)) => Err(Error::KeyOrphaned { name: name.clone() }),
            (false, None) => {
                let pair = self.key_store.generate(name)?;
                self.compute
                    .ensure_default_keypair(name, &pair.public)
                    .await
            }
        }
    }
}
