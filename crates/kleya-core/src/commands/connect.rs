use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::error::Error;
use crate::limits::{
    SSH_PROBE_INTERVAL_SECONDS, SSH_PROBE_PORT, SSH_PROBE_TCP_TIMEOUT_MS, SSH_PROBE_TIMEOUT_SECONDS,
};
use crate::model::instance::{Instance, InstanceFilter, InstanceId};
use crate::model::key::KeyName;
use crate::model::tag::{KLEYA_TAG_KEY, KLEYA_TAG_MANAGED};
use crate::parsed_config::ParsedConfig;
use crate::ports::cloud_compute::CloudCompute;
use crate::ports::key_store::KeyStore;
use crate::Result;
use once_cell::sync::Lazy;
use regex::Regex;

pub struct ConnectService {
    pub compute: Arc<dyn CloudCompute>,
    pub key_store: Arc<dyn KeyStore>,
    pub config: Arc<ParsedConfig>,
    pub region: String,
}

/// A fully-resolved ssh invocation. Program is required by the type so
/// callers cannot accidentally try to `exec` an empty argv.
#[derive(Debug, Clone)]
pub struct SshCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl SshCommand {
    /// Render as a single shell-quoted line — used by `--print`.
    #[must_use]
    pub fn shell_quote(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(|s| {
                if s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_/.@=:".contains(c))
                {
                    s.to_string()
                } else {
                    format!("'{}'", s.replace('\'', r"'\''"))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub struct ConnectPlan {
    pub command: SshCommand,
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
                return Err(Error::InvalidTmuxSession { name: s.clone() });
            }
        }
        let inst = self.resolve(opts).await?;
        let key_name = self.resolve_key(&inst)?;
        let key_path = self.key_store.private_path(&key_name)?;
        let endpoint = inst.public_dns.clone().ok_or_else(|| Error::NoPublicDns {
            instance: inst.id.clone(),
        })?;
        let command = self.build_command(&endpoint, &key_path, opts);
        Ok(ConnectPlan {
            command,
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
        let mut candidates = self
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
            1 => Ok(candidates.swap_remove(0)),
            _ => Err(Error::AmbiguousHandle {
                name: opts.handle.clone(),
                candidates: candidates.into_iter().map(|i| i.id).collect(),
            }),
        }
    }

    pub(crate) fn resolve_key(&self, inst: &Instance) -> Result<KeyName> {
        let tagged = inst
            .tags
            .iter()
            .find(|t| t.key() == KLEYA_TAG_KEY)
            .map(|t| t.value().to_string());
        if let Some(n) = tagged {
            return KeyName::new(n);
        }
        let managed = inst
            .tags
            .iter()
            .any(|t| t.key() == KLEYA_TAG_MANAGED && t.value() == "true");
        if !managed {
            return Err(Error::UnmanagedInstance {
                instance: inst.id.clone(),
            });
        }
        Ok(self.config.default_key_name.clone())
    }

    fn build_command(
        &self,
        endpoint: &str,
        key_path: &std::path::Path,
        opts: &ConnectOpts,
    ) -> SshCommand {
        let mut args: Vec<String> = vec![
            "-i".into(),
            key_path.to_string_lossy().into_owned(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-o".into(),
            "ServerAliveInterval=30".into(),
            "-o".into(),
            "ConnectTimeout=10".into(),
        ];
        // Override the pty's TERM so terminals whose terminfo is missing on the
        // remote (e.g. `xterm-ghostty`) don't break tmux/ncurses with
        // "missing or unsuitable terminal". Empty config keeps the local $TERM.
        if !self.config.ssh.term.is_empty() {
            args.push("-o".into());
            args.push(format!("SetEnv TERM={}", self.config.ssh.term));
        }
        for a in &self.config.ssh.extra_args {
            args.push(a.clone());
        }
        args.push("-t".into());
        args.push(format!("{}@{endpoint}", self.config.ssh.user));
        if !opts.no_tmux && self.config.ssh.tmux {
            let session = opts
                .tmux_session
                .clone()
                .unwrap_or_else(|| self.config.ssh.tmux_session.clone());
            args.push("tmux".into());
            args.push("new-session".into());
            args.push("-A".into());
            args.push("-s".into());
            args.push(session);
        }
        SshCommand {
            program: "ssh".into(),
            args,
        }
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
    use crate::config::Config;
    use crate::test_support::{FakeIdGen, InMemoryCompute, InMemoryKeyStore};

    fn parsed() -> Arc<ParsedConfig> {
        Arc::new(Config::default().parse().expect("default parses"))
    }

    #[tokio::test]
    async fn build_argv_includes_tmux_by_default() {
        let compute = Arc::new(InMemoryCompute::new());
        let key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let cfg = parsed();
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
            config: cfg.clone(),
        };
        l.run(LaunchOpts {
            template_name: None,
            instance_name: Some("box".into()),
            instance_type: None,
            market: None,
            dry_run: false,
            regenerate_key: false,
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
        assert_eq!(plan.command.program, "ssh");
        assert!(plan.command.args.iter().any(|a| a == "tmux"));
        assert!(
            plan.command
                .args
                .iter()
                .any(|a| a == "SetEnv TERM=xterm-256color"),
            "argv should set a remote-safe TERM by default"
        );
        let dash_s = plan
            .command
            .args
            .iter()
            .position(|a| a == "-s")
            .expect("args contains -s");
        assert_eq!(
            plan.command.args.get(dash_s + 1),
            Some(&cfg.ssh.tmux_session),
            "tmux session arg should match Config default"
        );
    }

    #[tokio::test]
    async fn resolve_key_rejects_unmanaged_instance() {
        use crate::model::instance::{Instance, InstanceId, InstanceState};
        use crate::model::tag::Tag;
        let cfg = parsed();
        let compute: Arc<dyn CloudCompute> = Arc::new(InMemoryCompute::new());
        let key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let svc = ConnectService {
            compute,
            key_store,
            config: cfg,
            region: "eu-west-1".into(),
        };
        let inst = Instance {
            id: InstanceId::new("i-aabbccdd").unwrap(),
            name: None,
            state: InstanceState::Running,
            public_dns: None,
            public_ip: None,
            tags: vec![Tag::new("Project", "other").unwrap()],
        };
        let err = svc.resolve_key(&inst).unwrap_err();
        assert!(matches!(err, Error::UnmanagedInstance { .. }));
    }
}
