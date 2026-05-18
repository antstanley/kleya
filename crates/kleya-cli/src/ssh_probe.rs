//! TCP-level SSH readiness probe — connect to port 22 with backoff until the
//! deadline. Spec §8: returns `Error::SshNotReady` on exhaustion.

use std::time::{Duration, Instant};

use kleya_core::limits::{
    SSH_PROBE_INTERVAL_SECONDS, SSH_PROBE_PORT, SSH_PROBE_TCP_TIMEOUT_MS, SSH_PROBE_TIMEOUT_SECONDS,
};
use kleya_core::model::instance::InstanceId;
use kleya_core::{Error, Result};

pub async fn probe_ssh_ready(
    endpoint: &str,
    instance: &InstanceId,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<()> {
    const { assert!(SSH_PROBE_INTERVAL_SECONDS > 0, "ssh probe interval is 0") };
    assert!(!endpoint.is_empty(), "ssh probe endpoint empty");
    let timeout = Duration::from_secs(u64::from(SSH_PROBE_TIMEOUT_SECONDS));
    let interval = Duration::from_secs(u64::from(SSH_PROBE_INTERVAL_SECONDS));
    let tcp_timeout = Duration::from_millis(u64::from(SSH_PROBE_TCP_TIMEOUT_MS));
    let addr = format!("{endpoint}:{SSH_PROBE_PORT}");
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(u64::from(u32::MAX)));
        if elapsed >= timeout {
            return Err(Error::SshNotReady {
                instance: instance.clone(),
                elapsed_seconds: u32::try_from(elapsed.as_secs()).unwrap_or(u32::MAX),
            });
        }
        let probe = tokio::time::timeout(tcp_timeout, tokio::net::TcpStream::connect(&addr)).await;
        if matches!(probe, Ok(Ok(_))) {
            return Ok(());
        }
        if kleya_core::util::wait_or_cancel(interval, Some(cancel)).await {
            return Err(Error::Cancelled {
                instance: instance.clone(),
            });
        }
    }
}

pub async fn wait_cloud_init(
    key_path: &std::path::Path,
    ssh_user: &str,
    endpoint: &str,
) -> Result<()> {
    let status = tokio::process::Command::new("ssh")
        .arg("-i")
        .arg(key_path)
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg(format!("{ssh_user}@{endpoint}"))
        .arg("cloud-init")
        .arg("status")
        .arg("--wait")
        .status()
        .await?;
    if !status.success() {
        return Err(Error::ConfigInvalid {
            reason: format!("cloud-init wait failed (exit {status})"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn returns_cancelled_when_token_pre_cancelled() {
        let tok = CancellationToken::new();
        tok.cancel();
        let id = InstanceId::new("i-cafe0001").unwrap();
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            probe_ssh_ready("no-such-host.invalid", &id, &tok),
        )
        .await
        .expect("test should not exceed 5s wall clock");
        match res {
            Err(Error::Cancelled { instance }) => assert_eq!(instance.as_str(), "i-cafe0001"),
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }
}
