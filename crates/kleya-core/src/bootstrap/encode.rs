use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write as _;

use crate::error::{Error, Result};
use crate::limits::{
    USER_DATA_BASE64_BYTES_MAX, USER_DATA_GZIP_BYTES_MAX, USER_DATA_RAW_BYTES_MAX,
};

pub fn encode_user_data(raw: &str) -> Result<String> {
    assert!(!raw.is_empty(), "encode_user_data called with empty raw");
    let raw_bytes = raw.len();
    if raw_bytes > USER_DATA_RAW_BYTES_MAX * 4 {
        return Err(Error::UserDataTooLarge {
            bytes: raw_bytes,
            max: USER_DATA_RAW_BYTES_MAX * 4,
        });
    }
    let mut enc = GzEncoder::new(Vec::with_capacity(raw_bytes), Compression::best());
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
    assert!(!b64.is_empty(), "encoded output empty");
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
    fn rejects_oversize_raw_input() {
        let big = "x".repeat(USER_DATA_RAW_BYTES_MAX * 4 + 1);
        let err = encode_user_data(&big).unwrap_err();
        assert!(matches!(err, Error::UserDataTooLarge { .. }));
    }

    #[test]
    fn boundary_at_raw_max_succeeds() {
        let at = "x".repeat(USER_DATA_RAW_BYTES_MAX);
        assert!(encode_user_data(&at).is_ok());
    }

    #[test]
    fn rejects_when_gzip_exceeds_cap() {
        // High-entropy input (random bytes) does not compress; pick a size that
        // stays under the raw-input ceiling (4×RAW) but exceeds GZIP after deflate.
        // 60_000 bytes of random data → gzip ~= 60_000 bytes (no compression).
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
}
