//! Small async utilities shared across the workspace.

#![allow(missing_docs)]

use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn wait_or_cancel(interval: Duration, cancel: Option<&CancellationToken>) -> bool {
    assert!(interval > Duration::ZERO, "wait_or_cancel interval is zero");
    if let Some(c) = cancel {
        tokio::select! {
            () = c.cancelled() => true,
            () = tokio::time::sleep(interval) => false,
        }
    } else {
        tokio::time::sleep(interval).await;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn elapsed_returns_false_when_no_cancel() {
        let res = wait_or_cancel(Duration::from_secs(5), None).await;
        assert!(!res);
    }

    #[tokio::test(start_paused = true)]
    async fn elapsed_returns_false_when_token_not_cancelled() {
        let tok = CancellationToken::new();
        let res = wait_or_cancel(Duration::from_secs(5), Some(&tok)).await;
        assert!(!res);
    }

    #[tokio::test(start_paused = true)]
    async fn returns_true_when_cancelled_before_sleep() {
        let tok = CancellationToken::new();
        tok.cancel();
        let res = wait_or_cancel(Duration::from_mins(1), Some(&tok)).await;
        assert!(res);
    }

    #[tokio::test(start_paused = true)]
    async fn returns_true_when_cancelled_during_sleep() {
        let tok = CancellationToken::new();
        let tok_for_task = tok.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            tok_for_task.cancel();
        });
        let res = wait_or_cancel(Duration::from_mins(1), Some(&tok)).await;
        assert!(res);
    }
}
