use kleya_core::Error;

/// Map a domain error to the process exit code documented in the spec.
///
/// Codes are stable for scripting consumers — see `docs/specs/07-error-model.md`.
#[must_use]
pub fn code_for(err: &Error) -> i32 {
    match err {
        Error::ConfigInvalid { .. }
        | Error::UserDataNotUtf8 { .. }
        | Error::BootstrapRender { .. }
        | Error::InvalidTmuxSession { .. }
        | Error::UnknownAmiAlias { .. } => 2,
        Error::InstanceNotFound { .. }
        | Error::TemplateNotFound { .. }
        | Error::TemplateNotFoundById { .. }
        | Error::NoDefaultVpc { .. }
        | Error::NoSubnetInDefaultVpc { .. } => 3,
        Error::AmbiguousHandle { .. } => 4,
        Error::SshNotReady { .. } => 5,
        Error::LaunchWaitTimeout { .. } => 6,
        Error::KeyMismatch { .. }
        | Error::KeyOrphaned { .. }
        | Error::KeyFileMode { .. }
        | Error::UnsupportedKeyAlgorithm => 7,
        Error::CloudInitFailed { .. } => 8,
        Error::NoPublicDns { .. } | Error::UnmanagedInstance { .. } => 9,
        Error::UserAborted => 10,
        Error::Cancelled { .. } => 130,
        Error::Adapter { .. } => 70,
        Error::Io(_) => 74,
        Error::UserDataTooLarge { .. } => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_maps_to_130() {
        let e = Error::Cancelled {
            instance: kleya_core::model::instance::InstanceId::new("i-cafef00d").unwrap(),
        };
        assert_eq!(code_for(&e), 130);
    }

    #[test]
    fn user_aborted_maps_to_10() {
        assert_eq!(code_for(&Error::UserAborted), 10);
    }

    #[test]
    fn cloud_init_maps_to_8() {
        assert_eq!(
            code_for(&Error::CloudInitFailed {
                status: "exit status 1".into()
            }),
            8
        );
    }

    #[test]
    fn no_public_dns_maps_to_9() {
        let e = Error::NoPublicDns {
            instance: kleya_core::model::instance::InstanceId::new("i-cafef00d").unwrap(),
        };
        assert_eq!(code_for(&e), 9);
    }
}
