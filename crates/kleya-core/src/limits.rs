//! Named bounds used throughout the workspace. Every magic number lives here.

#![allow(clippy::assertions_on_constants)]
#![allow(missing_docs)]

pub const CONFIG_BYTES_MAX: usize = 256 * 1024;
pub const USER_DATA_RAW_BYTES_MAX: usize = 16 * 1024;
pub const USER_DATA_GZIP_BYTES_MAX: usize = 16 * 1024;
pub const USER_DATA_BASE64_BYTES_MAX: usize = 21_848;
pub const TEMPLATES_COUNT_MAX: usize = 64;
pub const TAGS_PER_TEMPLATE_MAX: usize = 50;
pub const TAG_KEY_BYTES_MAX: usize = 128;
pub const TAG_VALUE_BYTES_MAX: usize = 256;
pub const INSTANCE_NAME_BYTES_MAX: usize = 63;
pub const KEY_NAME_BYTES_MAX: usize = 128;
pub const LAUNCH_WAIT_SECONDS_MAX: u32 = 600;
pub const LAUNCH_POLL_INTERVAL_SECONDS: u32 = 5;
pub const SSH_PROBE_PORT: u16 = 22;
pub const SSH_PROBE_TIMEOUT_SECONDS: u32 = 180;
pub const SSH_PROBE_INTERVAL_SECONDS: u32 = 3;
pub const SSH_PROBE_TCP_TIMEOUT_MS: u32 = 2_000;
pub const AWS_CALL_TIMEOUT_SECONDS: u32 = 30;
pub const AWS_RETRY_ATTEMPTS_MAX: u32 = 5;
pub const AWS_RETRY_BACKOFF_BASE_MS: u32 = 200;
pub const AWS_RETRY_BACKOFF_CAP_MS: u32 = 5_000;

const _: () = assert!(LAUNCH_POLL_INTERVAL_SECONDS <= LAUNCH_WAIT_SECONDS_MAX);
const _: () = assert!(SSH_PROBE_INTERVAL_SECONDS <= SSH_PROBE_TIMEOUT_SECONDS);
const _: () = assert!(AWS_RETRY_BACKOFF_BASE_MS <= AWS_RETRY_BACKOFF_CAP_MS);
const _: () = assert!(USER_DATA_GZIP_BYTES_MAX <= USER_DATA_RAW_BYTES_MAX);
const _: () = assert!(USER_DATA_BASE64_BYTES_MAX >= USER_DATA_RAW_BYTES_MAX);
const _: () = assert!(TAG_KEY_BYTES_MAX > 0 && TAG_VALUE_BYTES_MAX > 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_limits_are_aws_compatible() {
        assert_eq!(USER_DATA_RAW_BYTES_MAX, 16_384);
        assert_eq!(USER_DATA_GZIP_BYTES_MAX, 16_384);
        assert_eq!(USER_DATA_BASE64_BYTES_MAX, 21_848);
    }

    #[test]
    fn launch_wait_holds_at_least_one_full_interval() {
        assert!(LAUNCH_POLL_INTERVAL_SECONDS > 0);
        assert!(LAUNCH_WAIT_SECONDS_MAX / LAUNCH_POLL_INTERVAL_SECONDS >= 1);
    }
}
