//! One Error enum per crate, via `thiserror`. Adapter errors wrap in here.

#![allow(missing_docs)]

use crate::model::{instance::InstanceId, key::KeyName, template::TemplateName};
use std::path::PathBuf;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid value caught by `Config::validate` or domain-type construction.
    /// Reserve for true configuration-shape errors; specific failures get their
    /// own variant below.
    #[error("config invalid: {reason}")]
    ConfigInvalid { reason: String },

    #[error("user-data is too large: {bytes} > {max}")]
    UserDataTooLarge { bytes: usize, max: usize },

    #[error("user-data file is not valid utf-8: {reason}")]
    UserDataNotUtf8 { reason: String },

    #[error("instance not found: name={name} region={region}")]
    InstanceNotFound { name: String, region: String },

    #[error("ambiguous handle: {name} matches {} instances", .candidates.len())]
    AmbiguousHandle {
        name: String,
        candidates: Vec<InstanceId>,
    },

    #[error("ssh not ready after {elapsed_seconds}s for {instance}")]
    SshNotReady {
        instance: InstanceId,
        elapsed_seconds: u32,
    },

    #[error("launch timed out after {elapsed_seconds}s for {instance}")]
    LaunchWaitTimeout {
        instance: InstanceId,
        elapsed_seconds: u32,
    },

    #[error("cancelled: {instance}")]
    Cancelled { instance: InstanceId },

    #[error("ssh key mismatch for {name}: local fingerprint differs from EC2 record")]
    KeyMismatch { name: KeyName },

    #[error("ssh key orphaned: {name} is in EC2 but no local private key")]
    KeyOrphaned { name: KeyName },

    /// On-disk key file has unexpected permissions; we refuse to use it.
    #[error("ssh key file {} has mode 0o{mode:o}, expected 0o{want:o}", path.display())]
    KeyFileMode { path: PathBuf, mode: u32, want: u32 },

    /// Caller passed a key blob in an algorithm we don't support (e.g. RSA).
    #[error("unsupported ssh key algorithm: only Ed25519 is supported")]
    UnsupportedKeyAlgorithm,

    #[error("template not found: {name}")]
    TemplateNotFound { name: TemplateName },

    #[error("template not found by id: {id}")]
    TemplateNotFoundById {
        id: crate::model::template::TemplateId,
    },

    /// Instance is reachable but has no `public_dns_name` — it cannot be
    /// connected to without explicit endpoint hints.
    #[error("instance {instance} has no public DNS")]
    NoPublicDns { instance: InstanceId },

    /// Instance was found but is missing the `kleya:managed` tag, so we
    /// refuse to resolve a key for it.
    #[error("instance {instance} is not managed by kleya")]
    UnmanagedInstance { instance: InstanceId },

    /// Bootstrap user-data template rendering failed.
    #[error("bootstrap template render failed: {reason}")]
    BootstrapRender { reason: String },

    /// `cloud-init status --wait` returned non-zero on the remote instance.
    #[error("cloud-init wait failed: exit status {status}")]
    CloudInitFailed { status: String },

    /// User-supplied `--tmux-session` failed the safe-string check.
    #[error("invalid tmux session name '{name}'")]
    InvalidTmuxSession { name: String },

    /// User declined a confirmation prompt without `--yes`.
    #[error("aborted by user (pass --yes to confirm)")]
    UserAborted,

    /// CLI passed an `ami_alias` we don't have a parameter mapping for.
    #[error("unknown ami alias: {alias}")]
    UnknownAmiAlias { alias: String },

    /// No default VPC exists in the region.
    #[error("no default VPC in region {region}")]
    NoDefaultVpc { region: String },

    /// Default VPC exists but contains no subnets.
    #[error("no subnet in default VPC of region {region}")]
    NoSubnetInDefaultVpc { region: String },

    #[error("adapter {provider}: {source}")]
    Adapter {
        provider: &'static str,
        #[source]
        source: BoxError,
    },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    #[must_use]
    pub fn adapter<E>(provider: &'static str, source: E) -> Self
    where
        E: Into<BoxError>,
    {
        Self::Adapter {
            provider,
            source: source.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_handle_renders_count() {
        let e = Error::AmbiguousHandle {
            name: "devbox".into(),
            candidates: vec![
                InstanceId::new("i-deadbeef").unwrap(),
                InstanceId::new("i-cafef00d").unwrap(),
            ],
        };
        let s = format!("{e}");
        assert!(s.contains("devbox"));
        assert!(s.contains('2'));
    }

    #[test]
    fn user_data_too_large_renders_bytes_and_max() {
        let e = Error::UserDataTooLarge {
            bytes: 17_000,
            max: 16_384,
        };
        let s = format!("{e}");
        assert!(s.contains("17000"));
        assert!(s.contains("16384"));
    }

    #[test]
    fn cancelled_display_contains_instance() {
        let e = Error::Cancelled {
            instance: InstanceId::new("i-cafef00d").unwrap(),
        };
        let s = format!("{e}");
        assert!(s.contains("cancelled"), "got: {s}");
        assert!(s.contains("i-cafef00d"), "got: {s}");
    }

    #[test]
    fn user_aborted_renders() {
        let e = Error::UserAborted;
        assert!(format!("{e}").contains("aborted by user"));
    }

    #[test]
    fn no_public_dns_renders_instance() {
        let e = Error::NoPublicDns {
            instance: InstanceId::new("i-cafef00d").unwrap(),
        };
        assert!(format!("{e}").contains("i-cafef00d"));
    }
}
