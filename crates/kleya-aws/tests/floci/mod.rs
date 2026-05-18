#![allow(
    dead_code,
    missing_docs,
    clippy::expect_used,
    clippy::duration_suboptimal_units
)]

use std::process::Command;
use std::sync::OnceLock;

pub const FLOCI_ENDPOINT_ENV: &str = "KLEYA_TEST_FLOCI_ENDPOINT";
pub const FLOCI_ENABLE_ENV: &str = "KLEYA_TEST_FLOCI";
// Pinned to the floci/floci:latest manifest digest captured 2026-05-18 via
// `curl -H 'Accept: application/vnd.docker.distribution.manifest.v2+json' \
//   "https://registry-1.docker.io/v2/floci/floci/manifests/latest"`.
// Re-pin (and update this comment) when a newer Floci release is adopted.
pub const FLOCI_IMAGE: &str =
    "floci/floci@sha256:43f48b8cd04354f356b859cc43a8915a88516df6530d4691159bed39b7e9ea32";
pub const FLOCI_PORT: u16 = 4566;

static STARTED: OnceLock<()> = OnceLock::new();

pub fn ensure_floci() -> Option<String> {
    if std::env::var(FLOCI_ENABLE_ENV).is_err() {
        return None;
    }
    if FLOCI_IMAGE.contains("REPLACE_WITH_PIN") {
        eprintln!(
            "FLOCI_IMAGE digest is unpinned ({FLOCI_IMAGE}); pin it before \
             running floci tests"
        );
        return None;
    }
    STARTED.get_or_init(|| {
        let _ = Command::new("docker")
            .args(["rm", "-f", "kleya-floci"])
            .status();
        let status = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                "kleya-floci",
                "-p",
                &format!("{FLOCI_PORT}:{FLOCI_PORT}"),
                "-v",
                "/var/run/docker.sock:/var/run/docker.sock",
                FLOCI_IMAGE,
            ])
            .status()
            .expect("docker available");
        assert!(status.success(), "floci start failed");
        std::thread::sleep(std::time::Duration::from_millis(2000));
    });
    Some(
        std::env::var(FLOCI_ENDPOINT_ENV)
            .unwrap_or_else(|_| format!("http://localhost:{FLOCI_PORT}")),
    )
}

pub async fn ec2(endpoint: &str) -> aws_sdk_ec2::Client {
    kleya_aws::client::build_ec2_client("eu-west-1", Some(endpoint)).await
}
