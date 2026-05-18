use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::error::Error;
use crate::limits::{
    SSH_PROBE_INTERVAL_SECONDS, SSH_PROBE_PORT, SSH_PROBE_TCP_TIMEOUT_MS, SSH_PROBE_TIMEOUT_SECONDS,
};
use once_cell::sync::Lazy;
use regex::Regex;
use crate::model::instance::{Instance, InstanceFilter, InstanceId};
use crate::model::key::KeyName;
use crate::model::tag::KLEYA_TAG_KEY;
use crate::ports::cloud_compute::CloudCompute;
use crate::ports::key_store::KeyStore;
use crate::Result;

pub struct ConnectService {
    pub compute: Arc<dyn CloudCompute>,
    pub key_store: Arc<dyn KeyStore>,
    pub config: Arc<Config>,
    pub region: String,
}

pub struct ConnectPlan {
    pub argv: Vec<String>,
    pub instance_id: InstanceId,
    pub endpoint: String,
    pub key_path: PathBuf,
}

pub struct ConnectOpts {
    pub handle: String,
    pub explicit_instance_id: Option<String>,
    pub no_tmux: bool,
    pub tmux_session: Option<String>,
}

#[allow(clippy::expect_used, clippy::disallowed_methods)]
static TMUX_SESSION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9_-]{1,63}$").expect("static regex compiles"));

impl ConnectService {
    pub async fn plan(&self, opts: &ConnectOpts) -> Result<ConnectPlan> {
        if let Some(s) = &opts.tmux_session {
            if !TMUX_SESSION_RE.is_match(s) {
                return Err(Error::ConfigInvalid {
                    reason: format!(
                        "tmux session '{s}' must match ^[a-z0-9_-]{{1,63}}$"
                    ),
                });
            }
        }
        let inst = self.resolve(opts).await?;
        let key_name = self.resolve_key(&inst)?;
        let key_path = self.key_store.private_path(&key_name)?;
        let endpoint = inst
            .public_dns
            .clone()
            .ok_or_else(|| Error::ConfigInvalid {
                reason: format!("instance {} has no public DNS", inst.id.as_str()),
            })?;
        let argv = self.build_argv(&endpoint, &key_path, opts);
        Ok(ConnectPlan {
            argv,
            instance_id: inst.id,
            endpoint,
            key_path,
        })
    }

    async fn resolve(&self, opts: &ConnectOpts) -> Result<Instance> {
        if let Some(id) = &opts.explicit_instance_id {
            return self.compute.instance_describe(&InstanceId::new(id)?).await;
        }
        if opts.handle.starts_with("i-") {
            return self
                .compute
                .instance_describe(&InstanceId::new(&opts.handle)?)
                .await;
        }
        let candidates = self
            .compute
            .instance_list(&InstanceFilter {
                name: Some(opts.handle.clone()),
                managed_only: true,
                states: vec![],
            })
            .await?;
        match candidates.len() {
            0 => Err(Error::InstanceNotFound {
                name: opts.handle.clone(),
                region: self.region.clone(),
            }),
            1 => candidates
                .into_iter()
                .next()
                .ok_or_else(|| Error::ConfigInvalid {
                    reason: "candidates length 1 but iterator empty".into(),
                }),
            _ => Err(Error::AmbiguousHandle {
                name: opts.handle.clone(),
                candidates: candidates.into_iter().map(|i| i.id).collect(),
            }),
        }
    }

    fn resolve_key(&self, inst: &Instance) -> Result<KeyName> {
        let tagged = inst
            .tags
            .iter()
            .find(|t| t.key == KLEYA_TAG_KEY)
            .map(|t| t.value.clone());
        if let Some(n) = tagged {
            return KeyName::new(n);
        }
        let default = self.config.keys.default_key_name.trim();
        if default.is_empty() {
            return Err(Error::ConfigInvalid {
                reason: "no kleya:key tag on instance and no keys.default_key_name in config; \
                         re-launch via kleya launch or pass --instance-id with a known key"
                    .into(),
            });
        }
        KeyName::new(default)
    }

    fn build_argv(
        &self,
        endpoint: &str,
        key_path: &std::path::Path,
        opts: &ConnectOpts,
    ) -> Vec<String> {
        let mut argv: Vec<String> = vec!["ssh".into()];
        argv.push("-i".into());
        argv.push(key_path.to_string_lossy().into_owned());
        argv.push("-o".into());
        argv.push("StrictHostKeyChecking=accept-new".into());
        argv.push("-o".into());
        argv.push("ServerAliveInterval=30".into());
        argv.push("-o".into());
        argv.push("ConnectTimeout=10".into());
        for a in &self.config.ssh.extra_args {
            argv.push(a.clone());
        }
        argv.push("-t".into());
        argv.push(format!("{}@{endpoint}", self.config.ssh.user));
        if !opts.no_tmux && self.config.ssh.tmux {
            let session = opts
                .tmux_session
                .clone()
                .unwrap_or_else(|| self.config.ssh.tmux_session.clone());
            argv.push("tmux".into());
            argv.push("new-session".into());
            argv.push("-A".into());
            argv.push("-s".into());
            argv.push(session);
        }
        argv
    }
}

#[must_use]
pub fn probe_timing() -> (Duration, Duration, u16) {
    (
        Duration::from_secs(u64::from(SSH_PROBE_TIMEOUT_SECONDS)),
        Duration::from_secs(u64::from(SSH_PROBE_INTERVAL_SECONDS)),
        SSH_PROBE_PORT,
    )
}

pub const TCP_TIMEOUT_MS: u32 = SSH_PROBE_TCP_TIMEOUT_MS;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::commands::launch::{LaunchOpts, LaunchService};
    use crate::test_support::{FakeIdGen, InMemoryCompute, InMemoryKeyStore};

    #[tokio::test]
    async fn build_argv_includes_tmux_by_default() {
        let compute = Arc::new(InMemoryCompute::new());
        let key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let cfg = Arc::new(Config::default());
        let svc = ConnectService {
            compute: compute.clone(),
            key_store: key_store.clone(),
            config: cfg.clone(),
            region: "eu-west-1".into(),
        };
        let l = LaunchService {
            compute,
            key_store,
            id_gen: Arc::new(FakeIdGen::new()),
            config: cfg,
        };
        l.run(LaunchOpts {
            template_name: None,
            instance_name: Some("box".into()),
            instance_type: None,
            market: None,
            dry_run: false,
            cancel: None,
        })
        .await
        .unwrap();

        let plan = svc
            .plan(&ConnectOpts {
                handle: "box".into(),
                explicit_instance_id: None,
                no_tmux: false,
                tmux_session: None,
            })
            .await
            .expect("plan ok");
        assert!(plan.argv.iter().any(|a| a == "tmux"));
        assert!(plan.argv.iter().any(|a| a == "kleya"));
    }
}
