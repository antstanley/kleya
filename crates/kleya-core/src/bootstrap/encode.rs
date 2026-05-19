use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write as _;

use crate::error::{Error, Result};
use crate::limits::{
    USER_DATA_BASE64_BYTES_MAX, USER_DATA_GZIP_BYTES_MAX, USER_DATA_RAW_BYTES_MAX,
};

/// Compressed path: gzip then base64. The operative limit is `USER_DATA_GZIP_BYTES_MAX`
/// — EC2 stores the gzipped bytes as user-data and cloud-init detects gzip via the magic
/// header. The raw input has no hard cap from EC2 on this path, but we still refuse
/// pathologically large inputs to bound the gzip allocation.
pub fn encode_user_data(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(Error::ConfigInvalid {
            reason: "user-data is empty".into(),
        });
    }
    if raw.len() > USER_DATA_RAW_BYTES_MAX * 4 {
        return Err(Error::UserDataTooLarge {
            bytes: raw.len(),
            max: USER_DATA_RAW_BYTES_MAX * 4,
        });
    }
    let mut enc = GzEncoder::new(Vec::with_capacity(raw.len()), Compression::best());
    enc.write_all(raw.as_bytes())?;
    let gz = enc.finish()?;
    if gz.len() > USER_DATA_GZIP_BYTES_MAX {
        return Err(Error::UserDataTooLarge {
            bytes: gz.len(),
            max: USER_DATA_GZIP_BYTES_MAX,
        });
    }
    let b64 = B64.encode(&gz);
    debug_assert!(
        b64.len() <= USER_DATA_BASE64_BYTES_MAX,
        "base64 of gzipped data must respect the derived ceiling"
    );
    debug_assert!(!b64.is_empty());
    Ok(b64)
}

/// Uncompressed path: caller supplied an opaque script via `bootstrap.user_data_path` /
/// `--user-data <path>`. Per spec §7 we **still apply the size + encoding checks** but
/// skip templating. The operative limit is `USER_DATA_RAW_BYTES_MAX` (16 KiB) since
/// nothing is gzipped on the wire.
pub fn encode_user_data_passthrough(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(Error::ConfigInvalid {
            reason: "user-data is empty".into(),
        });
    }
    if raw.len() > USER_DATA_RAW_BYTES_MAX {
        return Err(Error::UserDataTooLarge {
            bytes: raw.len(),
            max: USER_DATA_RAW_BYTES_MAX,
        });
    }
    let b64 = B64.encode(raw.as_bytes());
    debug_assert!(
        b64.len() <= USER_DATA_BASE64_BYTES_MAX,
        "base64 of raw data must respect the derived ceiling"
    );
    debug_assert!(!b64.is_empty());
    Ok(b64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_small_script() {
        let s = encode_user_data("#!/usr/bin/env bash\necho hi\n").expect("encodes");
        assert!(!s.is_empty());
    }

    #[test]
    fn boundary_at_raw_max_succeeds() {
        let at = "x".repeat(USER_DATA_RAW_BYTES_MAX);
        assert!(encode_user_data(&at).is_ok());
    }

    #[test]
    fn rejects_when_gzip_exceeds_cap() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rng_like = String::with_capacity(60_000);
        for _ in 0..60_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let b = ((state >> 33) & 0x7F) as u32;
            rng_like.push(char::from_u32(33 + (b % 90)).unwrap_or('x'));
        }
        let err = encode_user_data(&rng_like).unwrap_err();
        assert!(matches!(err, Error::UserDataTooLarge { .. }));
    }

    #[test]
    fn passthrough_rejects_oversize_raw_input() {
        let big = "x".repeat(USER_DATA_RAW_BYTES_MAX + 1);
        let err = encode_user_data_passthrough(&big).unwrap_err();
        assert!(matches!(err, Error::UserDataTooLarge { .. }));
    }

    #[test]
    fn passthrough_at_limit_succeeds() {
        let at = "x".repeat(USER_DATA_RAW_BYTES_MAX);
        assert!(encode_user_data_passthrough(&at).is_ok());
    }

    #[test]
    fn rejects_oversize_raw_input() {
        let big = "x".repeat(USER_DATA_RAW_BYTES_MAX * 4 + 1);
        let err = encode_user_data(&big).unwrap_err();
        match err {
            Error::UserDataTooLarge { bytes, max } => {
                assert_eq!(max, USER_DATA_RAW_BYTES_MAX * 4);
                assert_eq!(bytes, USER_DATA_RAW_BYTES_MAX * 4 + 1);
            }
            other => panic!("expected UserDataTooLarge with raw ceiling, got {other:?}"),
        }
    }
}
