//! One Error enum per crate, via `thiserror`. Adapter errors wrap in here.

#![allow(missing_docs)]

use crate::model::{instance::InstanceId, key::KeyName};

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config invalid: {reason}")]
    ConfigInvalid { reason: String },

    #[error("user-data is too large: {bytes} > {max}")]
    UserDataTooLarge { bytes: usize, max: usize },

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

    #[error("ssh key mismatch for {name}: local fingerprint differs from EC2 record")]
    KeyMismatch { name: KeyName },

    #[error("ssh key orphaned: {name} is in EC2 but no local private key")]
    KeyOrphaned { name: KeyName },

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
}
