//! One Error enum per crate, via `thiserror`. Adapter errors wrap in here.

#![allow(missing_docs)]

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config invalid: {reason}")]
    ConfigInvalid { reason: String },

    #[error("user-data is too large: {bytes} > {max} bytes")]
    UserDataTooLarge { bytes: usize, max: usize },

    #[error("instance not found: name={name} region={region}")]
    InstanceNotFound { name: String, region: String },

    #[error("ambiguous handle: {name} matches {count} instances")]
    AmbiguousHandle {
        name: String,
        count: usize,
        candidates: Vec<String>,
    },

    #[error("ssh not ready after {elapsed_seconds}s for instance={instance_id}")]
    SshNotReady {
        instance_id: String,
        elapsed_seconds: u32,
    },

    #[error("launch wait timed out after {elapsed_seconds}s for instance={instance_id}")]
    LaunchWaitTimeout {
        instance_id: String,
        elapsed_seconds: u32,
    },

    #[error("ssh key mismatch for {name}: local fingerprint differs from cloud record")]
    KeyMismatch { name: String },

    #[error("ssh key orphaned: {name} is registered with provider but no local private key")]
    KeyOrphaned { name: String },

    #[error("adapter {provider}: {source}")]
    Adapter {
        provider: &'static str,
        #[source]
        source: BoxError,
    },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("template render: {0}")]
    TemplateRender(String),
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
            count: 2,
            candidates: vec!["i-1".into(), "i-2".into()],
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
