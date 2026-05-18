use kleya_core::Error;

#[must_use]
pub fn code_for(err: &Error) -> i32 {
    match err {
        Error::ConfigInvalid { .. } => 2,
        Error::InstanceNotFound { .. } => 3,
        Error::AmbiguousHandle { .. } => 4,
        Error::SshNotReady { .. } => 5,
        Error::LaunchWaitTimeout { .. } => 6,
        Error::KeyMismatch { .. } | Error::KeyOrphaned { .. } => 7,
        Error::Adapter { .. } => 70,
        Error::Io(_) => 74,
        Error::UserDataTooLarge { .. } => 1,
    }
}
