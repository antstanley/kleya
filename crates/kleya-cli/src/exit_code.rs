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
}
