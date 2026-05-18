# Review-Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all eight findings from the review of the pending `kleya` branch (cancellation semantics, encode preflight, connect managed gate, `--connect` implication rule, async file I/O, CLI surface cleanups).

**Architecture:** Fixes are applied on top of the existing uncommitted work in `@`. Each task becomes one jj revision with a Conventional Commit message; the user can squash/reorder at merge time. A new `wait_or_cancel` helper in `kleya-core::util` is introduced so the two poll loops (ssh probe + EC2 instance wait) share a tested cancellation pattern.

**Tech Stack:** Rust 1.95.0, tokio, tokio-util, thiserror, clap. Test runner: `cargo nextest`. VCS: jj on git backend.

**Per-task commit convention:** Conventional Commits (`fix(scope): …`, `feat(scope): …`, `refactor(scope): …`). After every task's `Run tests` step passes, run `jj describe -m "<subject>"` then `jj new`. Never `jj abandon` or `--no-verify`. Do not push.

---

## Task 1: Add `Error::Cancelled` variant + exit code 130

**Files:**
- Modify: `crates/kleya-core/src/error.rs`
- Modify: `crates/kleya-cli/src/exit_code.rs`

- [ ] **Step 1: Write the failing test in `error.rs`**

Append to the existing `#[cfg(test)] mod tests` block at the bottom of `crates/kleya-core/src/error.rs`:

```rust
#[test]
fn cancelled_display_contains_instance() {
    let e = Error::Cancelled {
        instance: InstanceId::new("i-cafef00d").unwrap(),
    };
    let s = format!("{e}");
    assert!(s.contains("cancelled"), "got: {s}");
    assert!(s.contains("i-cafef00d"), "got: {s}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p kleya-core error::tests::cancelled_display_contains_instance`
Expected: build error — `Cancelled` variant not found.

- [ ] **Step 3: Add the variant**

In `crates/kleya-core/src/error.rs`, inside `pub enum Error`, add (place it just above the `Adapter` arm):

```rust
#[error("cancelled: {instance}")]
Cancelled { instance: InstanceId },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p kleya-core error::tests::cancelled_display_contains_instance`
Expected: PASS.

- [ ] **Step 5: Write the failing exit-code test**

Append to the existing `#[cfg(test)] mod tests` block at the bottom of `crates/kleya-cli/src/exit_code.rs`:

```rust
#[test]
fn cancelled_maps_to_130() {
    let e = Error::Cancelled {
        instance: kleya_core::model::instance::InstanceId::new("i-1").unwrap(),
    };
    assert_eq!(code_for(&e), 130);
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo nextest run -p kleya-cli exit_code::tests::cancelled_maps_to_130`
Expected: build error or wrong exit code (the match has no arm for `Cancelled`).

- [ ] **Step 7: Add the exit-code arm**

In `crates/kleya-cli/src/exit_code.rs`, inside `pub fn code_for`, add a new arm (place above the `Adapter` arm):

```rust
Error::Cancelled { .. } => 130,
```

- [ ] **Step 8: Run all tests in the two crates**

Run: `cargo nextest run -p kleya-core -p kleya-cli`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
jj describe -m "feat(core): add Error::Cancelled variant for SIGINT (exit 130)"
jj new
```

---

## Task 2: Add `wait_or_cancel` helper in `kleya-core::util`

A small shared helper used by both poll loops in Tasks 3 and 4. Returns `true` if cancelled, `false` if the interval elapsed. Tested in isolation against a paused tokio runtime.

**Files:**
- Create: `crates/kleya-core/src/util.rs`
- Modify: `crates/kleya-core/src/lib.rs`

- [ ] **Step 1: Add the module declaration**

In `crates/kleya-core/src/lib.rs`, add (after the existing `pub mod` lines, alphabetically near `pub mod test_support`):

```rust
pub mod util;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/kleya-core/src/util.rs` with the test module only (no implementation yet):

```rust
//! Small async utilities shared across the workspace.

#![allow(missing_docs)]

use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn wait_or_cancel(
    interval: Duration,
    cancel: Option<&CancellationToken>,
) -> bool {
    todo!()
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
        let res = wait_or_cancel(Duration::from_secs(60), Some(&tok)).await;
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
        let res = wait_or_cancel(Duration::from_secs(60), Some(&tok)).await;
        assert!(res);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo nextest run -p kleya-core util::tests`
Expected: tests panic on `todo!()`.

- [ ] **Step 4: Replace the `todo!()` body with the real implementation**

In `crates/kleya-core/src/util.rs`, replace the function body:

```rust
pub async fn wait_or_cancel(
    interval: Duration,
    cancel: Option<&CancellationToken>,
) -> bool {
    assert!(interval > Duration::ZERO, "wait_or_cancel interval is zero");
    match cancel {
        Some(c) => tokio::select! {
            () = c.cancelled() => true,
            () = tokio::time::sleep(interval) => false,
        },
        None => {
            tokio::time::sleep(interval).await;
            false
        }
    }
}
```

- [ ] **Step 5: Ensure kleya-core has `tokio` as a dev-dep with the test runtime**

Check `crates/kleya-core/Cargo.toml` — the existing `[dev-dependencies] tokio = { workspace = true, features = ["macros", "rt"] }` is needed for `#[tokio::test]`. The `start_paused = true` attribute requires the `test-util` feature on tokio.

If absent, add `test-util` to the dev-dep features:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt", "test-util"] }
```

(Check first; only modify if `test-util` is missing.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo nextest run -p kleya-core util::tests`
Expected: 4 tests PASS.

- [ ] **Step 7: Commit**

```bash
jj describe -m "feat(core): add wait_or_cancel helper for poll loops"
jj new
```

---

## Task 3: Use helper in `ssh_probe` (add `cancel` param, responsive sleep)

**Files:**
- Modify: `crates/kleya-cli/src/ssh_probe.rs`
- Modify: `crates/kleya-cli/src/dispatch.rs`

- [ ] **Step 0: Ensure `test-util` is on the kleya-cli dev tokio**

In `crates/kleya-cli/Cargo.toml`, find:
```toml
[dev-dependencies]
tempfile = "3"
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
```
Change the tokio line to:
```toml
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
```

- [ ] **Step 1: Write the failing cancellation test**

Append to `crates/kleya-cli/src/ssh_probe.rs` (add a `#[cfg(test)] mod tests` at the bottom if not present):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    #[tokio::test(start_paused = true)]
    async fn returns_cancelled_when_token_fires() {
        let tok = CancellationToken::new();
        let tok_for_task = tok.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tok_for_task.cancel();
        });
        // Use a clearly-refusing endpoint (port 1 typically refused or filtered).
        let id = InstanceId::new("i-cancel-test").unwrap();
        let res = probe_ssh_ready("127.0.0.1", &id, &tok).await;
        match res {
            Err(Error::Cancelled { instance }) => assert_eq!(instance.as_str(), "i-cancel-test"),
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }
}
```

Note: the test uses `127.0.0.1` as the endpoint but the probe builds `"127.0.0.1:22"`. If sshd is not running locally the connect will fail fast; the sleep loop is what we want to cancel. `start_paused = true` makes `tokio::time::sleep` deterministic.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p kleya-cli ssh_probe::tests::returns_cancelled_when_token_fires`
Expected: build error — `probe_ssh_ready` signature mismatch (no `cancel` param).

- [ ] **Step 3: Update `probe_ssh_ready` to take `cancel` and use the helper**

Replace the entire body of `probe_ssh_ready` in `crates/kleya-cli/src/ssh_probe.rs`:

```rust
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
            return Err(Error::Cancelled { instance: instance.clone() });
        }
    }
}
```

- [ ] **Step 4: Update call sites in `dispatch.rs`**

In `crates/kleya-cli/src/dispatch.rs`, find the two call sites and pass `&cancel`:

The `Cmd::Launch` arm:
```rust
crate::ssh_probe::probe_ssh_ready(&endpoint, &inst.id, &cancel).await?;
```

The `Cmd::Connect` arm (the `probe_ssh_ready` call just before the `Command::new(...).exec()`):
```rust
crate::ssh_probe::probe_ssh_ready(&plan.endpoint, &plan.instance_id, &cancel).await?;
```

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p kleya-cli`
Expected: all PASS (including the new cancellation test).

- [ ] **Step 6: Commit**

```bash
jj describe -m "fix(cli): respond to cancel token in probe_ssh_ready"
jj new
```

---

## Task 4: Update `ec2::instance_wait_running` to return `Cancelled` via the helper

**Files:**
- Modify: `crates/kleya-aws/src/ec2.rs`

- [ ] **Step 1: Update the loop**

In `crates/kleya-aws/src/ec2.rs::AwsEc2::instance_wait_running`, replace the current loop body. Final loop (preserving the asserts and `max_attempts` belt):

```rust
async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance> {
    assert!(deadline.poll_interval.as_secs() > 0, "poll_interval is 0");
    assert!(deadline.timeout > deadline.poll_interval, "timeout < poll");
    let start = std::time::Instant::now();
    let max_attempts: u32 = u32::try_from(
        deadline.timeout.as_secs() / deadline.poll_interval.as_secs() + 2,
    )
    .unwrap_or(u32::MAX);
    let mut attempts: u32 = 0;
    loop {
        attempts = attempts.saturating_add(1);
        let inst = self.instance_describe(id).await?;
        if matches!(inst.state, InstanceState::Running) {
            return Ok(inst);
        }
        if start.elapsed() >= deadline.timeout || attempts >= max_attempts {
            return Err(kleya_core::Error::LaunchWaitTimeout {
                instance: id.clone(),
                elapsed_seconds: u32::try_from(start.elapsed().as_secs()).unwrap_or(u32::MAX),
            });
        }
        if kleya_core::util::wait_or_cancel(deadline.poll_interval, deadline.cancel.as_ref()).await {
            return Err(kleya_core::Error::Cancelled { instance: id.clone() });
        }
    }
}
```

Notes:
- The top-of-loop `if let Some(c) = &deadline.cancel { if c.is_cancelled() { ... } }` block is removed — `wait_or_cancel` now covers it (a pre-cancelled token resolves immediately inside `tokio::select!`).
- The `LaunchWaitTimeout` arm no longer masks cancellations.

- [ ] **Step 2: Compile-check**

Run: `cargo check -p kleya-aws`
Expected: PASS.

- [ ] **Step 3: Run workspace tests (no aws-specific unit test exists; we rely on workspace coverage)**

Run: `cargo nextest run --workspace`
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
jj describe -m "fix(aws): return Cancelled and respond to cancel in instance_wait_running"
jj new
```

---

## Task 5: Restore `encode_user_data` raw-input preflight

**Files:**
- Modify: `crates/kleya-core/src/bootstrap/encode.rs`

- [ ] **Step 1: Re-add the failing preflight test**

In the `#[cfg(test)] mod tests` block at the bottom of `crates/kleya-core/src/bootstrap/encode.rs`, add:

```rust
#[test]
fn rejects_oversize_raw_input() {
    let big = "x".repeat(USER_DATA_RAW_BYTES_MAX * 4 + 1);
    let err = encode_user_data(&big).unwrap_err();
    assert!(matches!(err, Error::UserDataTooLarge { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p kleya-core bootstrap::encode::tests::rejects_oversize_raw_input`
Expected: FAIL — current `encode_user_data` allocates and gzips the input; the test would either pass for the wrong reason (gzip cap) or hit OOM. If it passes via gzip cap, we still want the cheaper preflight; assert on the `bytes` field to lock in the exact ceiling.

To make the failure explicit (rules out the gzip path), strengthen the assertion:

```rust
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
```

Now the test fails because `max` will be `USER_DATA_GZIP_BYTES_MAX`, not `USER_DATA_RAW_BYTES_MAX * 4`.

- [ ] **Step 3: Restore the preflight**

In `crates/kleya-core/src/bootstrap/encode.rs::encode_user_data`, just after `assert!(!raw.is_empty(), ...)`, insert:

```rust
if raw.len() > USER_DATA_RAW_BYTES_MAX * 4 {
    return Err(Error::UserDataTooLarge {
        bytes: raw.len(),
        max: USER_DATA_RAW_BYTES_MAX * 4,
    });
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p kleya-core bootstrap::encode::tests`
Expected: all encode tests PASS.

- [ ] **Step 5: Commit**

```bash
jj describe -m "fix(core): restore raw-input preflight in encode_user_data"
jj new
```

---

## Task 6: Restore `Connect::resolve_key` managed-instance gate

**Files:**
- Modify: `crates/kleya-core/src/commands/connect.rs`

- [ ] **Step 1: Write the failing test**

In `crates/kleya-core/src/commands/connect.rs`, find the existing `#[cfg(test)] mod tests` block. Add this test (it constructs an instance without the `kleya:managed=true` tag and asserts rejection):

```rust
#[tokio::test]
async fn resolve_key_rejects_unmanaged_instance() {
    use crate::model::instance::{Instance, InstanceId, InstanceState};
    use crate::model::tag::Tag;
    let cfg = Arc::new(Config::default());
    let compute: Arc<dyn CloudCompute> = Arc::new(InMemoryCompute::new());
    let key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
    let svc = ConnectService {
        compute,
        key_store,
        config: cfg,
        region: "eu-west-1".into(),
    };
    let inst = Instance {
        id: InstanceId::new("i-unmanaged").unwrap(),
        name: None,
        state: InstanceState::Running,
        public_dns: None,
        public_ip: None,
        tags: vec![Tag::new("Project", "other").unwrap()],
    };
    let err = svc.resolve_key(&inst).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("not managed by kleya"), "got: {msg}");
}
```

Note: `resolve_key` is currently a private associated fn. Either:
- (a) make it `pub(crate)` for testability, or
- (b) test indirectly via `plan()`.

Prefer (a) — narrow visibility bump. Add `pub(crate)` to `resolve_key` in the same step.

- [ ] **Step 2: Adjust visibility for testability**

In `crates/kleya-core/src/commands/connect.rs`, change:
```rust
fn resolve_key(&self, inst: &Instance) -> Result<KeyName> {
```
to:
```rust
pub(crate) fn resolve_key(&self, inst: &Instance) -> Result<KeyName> {
```

Check fields used by the test (`Instance` struct fields) match what's defined in `crates/kleya-core/src/model/instance.rs`. If any field name differs, adjust the test literal.

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo nextest run -p kleya-core commands::connect::tests::resolve_key_rejects_unmanaged_instance`
Expected: FAIL — current code does not enforce the managed gate; it falls through to `default_key_name`.

- [ ] **Step 4: Restore the managed-instance gate**

In `crates/kleya-core/src/commands/connect.rs`, change the `KLEYA_TAG_KEY` import to also pull `KLEYA_TAG_MANAGED`:

```rust
use crate::model::tag::{KLEYA_TAG_KEY, KLEYA_TAG_MANAGED};
```

Replace the body of `resolve_key` (the section after the `tagged` lookup, replacing the empty-default-key check with the managed gate first):

```rust
pub(crate) fn resolve_key(&self, inst: &Instance) -> Result<KeyName> {
    let tagged = inst
        .tags
        .iter()
        .find(|t| t.key == KLEYA_TAG_KEY)
        .map(|t| t.value.clone());
    if let Some(n) = tagged {
        return KeyName::new(n);
    }
    let managed = inst
        .tags
        .iter()
        .any(|t| t.key == KLEYA_TAG_MANAGED && t.value == "true");
    if !managed {
        return Err(Error::ConfigInvalid {
            reason: format!(
                "instance {} not managed by kleya; pass --instance-id and configure a key",
                inst.id.as_str()
            ),
        });
    }
    let default = self.config.keys.default_key_name.trim();
    if default.is_empty() {
        return Err(Error::ConfigInvalid {
            reason: "no kleya:key tag on instance and no keys.default_key_name in config; \
                     re-launch via kleya launch or pass --instance-id with a known key"
                .into(),
        });
    }
    KeyName::new(default)
}
```

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p kleya-core commands::connect`
Expected: all PASS (the existing tests use launches that set `KLEYA_TAG_MANAGED=true` via `LaunchService`, so they should still resolve).

- [ ] **Step 6: Commit**

```bash
jj describe -m "fix(core): restore managed-instance gate in Connect::resolve_key"
jj new
```

---

## Task 7: `--connect` implies cloud-init wait; add `--no-wait-bootstrap`

**Files:**
- Modify: `crates/kleya-cli/src/clap_args.rs`
- Modify: `crates/kleya-cli/src/dispatch.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/kleya-cli/src/dispatch.rs`, append (or extend) the `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::clap_args::LaunchArgs;

    fn args(connect: bool, wait: bool, no_wait: bool) -> LaunchArgs {
        LaunchArgs {
            template: None,
            name: None,
            instance_type: None,
            market: None,
            connect,
            wait_bootstrap: wait,
            no_wait_bootstrap: no_wait,
            dry_run: false,
        }
    }

    #[test]
    fn effective_wait_explicit_flag_wins() {
        assert!(effective_wait_bootstrap(&args(false, true, false)));
        assert!(effective_wait_bootstrap(&args(false, true, true))); // explicit wait beats no-wait
    }

    #[test]
    fn effective_wait_connect_implies_wait() {
        assert!(effective_wait_bootstrap(&args(true, false, false)));
    }

    #[test]
    fn effective_wait_connect_with_no_wait_skips() {
        assert!(!effective_wait_bootstrap(&args(true, false, true)));
    }

    #[test]
    fn effective_wait_neither_default_false() {
        assert!(!effective_wait_bootstrap(&args(false, false, false)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p kleya-cli dispatch::tests`
Expected: build error — `effective_wait_bootstrap` not found, `no_wait_bootstrap` field not on `LaunchArgs`.

- [ ] **Step 3: Add `no_wait_bootstrap` to `LaunchArgs`**

In `crates/kleya-cli/src/clap_args.rs`, in the `pub struct LaunchArgs` definition, add after the existing `wait_bootstrap` field:

```rust
#[arg(long)]
pub no_wait_bootstrap: bool,
```

- [ ] **Step 4: Add the `effective_wait_bootstrap` helper**

In `crates/kleya-cli/src/dispatch.rs`, add this helper near the other free helpers (`shell_quote`, `confirm`):

```rust
pub(crate) fn effective_wait_bootstrap(args: &crate::clap_args::LaunchArgs) -> bool {
    args.wait_bootstrap || (args.connect && !args.no_wait_bootstrap)
}
```

- [ ] **Step 5: Use the helper in the `Cmd::Launch` arm**

In `crates/kleya-cli/src/dispatch.rs::run_with`, inside `Cmd::Launch`, replace the existing `if args.wait_bootstrap { ... }` block with one driven by the effective rule. Specifically replace this block:

```rust
if args.wait_bootstrap {
    let key_name = inst.tags.iter().find(|t| t.key == "kleya:key").map_or_else(
        || config.keys.default_key_name.clone(),
        |t| t.value.clone(),
    );
    let key = kleya_core::model::key::KeyName::new(key_name)?;
    let key_path = key_store.private_path(&key)?;
    crate::ssh_probe::wait_cloud_init(
        &key_path,
        &config.ssh.user,
        &endpoint,
    )
    .await?;
}
```

with:

```rust
if effective_wait_bootstrap(&args) {
    let key_name = inst.tags.iter().find(|t| t.key == "kleya:key").map_or_else(
        || config.keys.default_key_name.clone(),
        |t| t.value.clone(),
    );
    let key = kleya_core::model::key::KeyName::new(key_name)?;
    let key_path = key_store.private_path(&key)?;
    crate::ssh_probe::wait_cloud_init(
        &key_path,
        &config.ssh.user,
        &endpoint,
    )
    .await?;
}
```

The surrounding `if args.connect || args.wait_bootstrap { ... }` outer gate is also relaxed — the SSH probe should also run when the new effective rule fires. Update the outer condition:

```rust
if args.connect || effective_wait_bootstrap(&args) {
```

- [ ] **Step 6: Run tests**

Run: `cargo nextest run -p kleya-cli`
Expected: 4 new tests PASS; existing tests still PASS.

- [ ] **Step 7: Commit**

```bash
jj describe -m "feat(cli): --connect implies bootstrap wait; add --no-wait-bootstrap"
jj new
```

---

## Task 8: Async file I/O for `build_template_spec` and `render_user_data`

**Files:**
- Modify: `crates/kleya-cli/src/dispatch.rs`
- Modify: `crates/kleya-core/src/commands/launch.rs`

- [ ] **Step 1: Make `build_template_spec` async, switch to `tokio::fs::read`**

In `crates/kleya-cli/src/dispatch.rs`, change the function signature and the file-read line:

```rust
async fn build_template_spec(
    config: &Arc<Config>,
    name: &str,
    args: &crate::clap_args::TemplateCreateArgs,
) -> kleya_core::Result<kleya_core::model::template::TemplateSpec> {
    // ...
    let user_data_b64 = if let Some(path) = &args.user_data {
        let bytes = tokio::fs::read(path).await?;
        // ... rest unchanged
    };
    // ...
}
```

- [ ] **Step 2: Update the two call sites to `.await`**

In `crates/kleya-cli/src/dispatch.rs`, the `TemplateCmd::Create` and `TemplateCmd::Update` arms each have:
```rust
let spec = build_template_spec(&config, &args.name, &args)?;
```
or
```rust
let spec = build_template_spec(&config, &args.name, &create_args)?;
```

Change both to:
```rust
let spec = build_template_spec(&config, &args.name, &args).await?;
```
and
```rust
let spec = build_template_spec(&config, &args.name, &create_args).await?;
```

- [ ] **Step 3: Make `render_user_data` async, switch to `tokio::fs::read`**

In `crates/kleya-core/src/commands/launch.rs`, change:

```rust
fn render_user_data(&self) -> Result<String> {
    // ...
    let bytes = std::fs::read(&expanded)?;
    // ...
}
```

to:

```rust
async fn render_user_data(&self) -> Result<String> {
    // ...
    let bytes = tokio::fs::read(&expanded).await?;
    // ...
}
```

- [ ] **Step 4: Update the caller (`ensure_template`) to `.await`**

In `crates/kleya-core/src/commands/launch.rs::LaunchService::ensure_template`, change:
```rust
let user_data_b64 = self.render_user_data()?;
```
to:
```rust
let user_data_b64 = self.render_user_data().await?;
```

- [ ] **Step 5: Add `tokio` as a non-dev dep on `kleya-core`; ensure `fs` feature is in workspace tokio**

`kleya-core` currently has tokio only as a dev-dep. We now need `tokio::fs::read` in production code, so:

1. In the workspace root `Cargo.toml`, add `"fs"` to the tokio features list. Change:
```toml
tokio = { version = "1", features = ["macros", "rt", "time", "sync", "net", "process", "signal"] }
```
to:
```toml
tokio = { version = "1", features = ["macros", "rt", "time", "sync", "net", "process", "signal", "fs"] }
```

2. In `crates/kleya-core/Cargo.toml`, add tokio to `[dependencies]` (just below the existing `tokio-util` line):
```toml
tokio       = { workspace = true }
```

The existing `[dev-dependencies] tokio = { workspace = true, features = ["macros", "rt"] }` entry stays — dev-deps merge their features on top of the non-dev features. Cargo unifies features across the build graph, so `kleya-core` non-test code sees the full workspace feature set including `fs`.

- [ ] **Step 6: Compile-check workspace**

Run: `cargo check --workspace --all-targets`
Expected: PASS.

- [ ] **Step 7: Run workspace tests**

Run: `cargo nextest run --workspace`
Expected: all PASS.

- [ ] **Step 8: Commit**

```bash
jj describe -m "refactor(core): async file I/O for user-data reads"
jj new
```

---

## Task 9: `TemplateUpdate --name` flag + skip empty SG list

Two unrelated micro-cleanups bundled into one commit.

**Files:**
- Modify: `crates/kleya-cli/src/clap_args.rs`
- Modify: `crates/kleya-aws/src/ec2.rs`

- [ ] **Step 1: Make `TemplateUpdateArgs.name` a `--name` flag**

In `crates/kleya-cli/src/clap_args.rs`, in `pub struct TemplateUpdateArgs`:

Before:
```rust
pub struct TemplateUpdateArgs {
    pub name: String,
    #[arg(long)]
    pub ami: Option<String>,
    // ...
}
```

After:
```rust
pub struct TemplateUpdateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub ami: Option<String>,
    // ...
}
```

- [ ] **Step 2: Skip empty `set_security_group_ids`**

In `crates/kleya-aws/src/ec2.rs::build_request_launch_template_data`, replace the unconditional `.set_security_group_ids(...)` chain:

Before:
```rust
let mut data = e::RequestLaunchTemplateData::builder()
    .instance_type(e::InstanceType::from(spec.instance_type.as_str()))
    .key_name(spec.key_name.as_str())
    .user_data(&spec.user_data_base64)
    .set_security_group_ids(Some(
        spec.security_group_ids
            .iter()
            .map(|s| s.0.clone())
            .collect(),
    ))
    .set_tag_specifications(Some(vec![
        e::LaunchTemplateTagSpecificationRequest::builder()
            .resource_type(e::ResourceType::Instance)
            .set_tags(Some(tags))
            .build(),
    ]));
```

After:
```rust
let mut data = e::RequestLaunchTemplateData::builder()
    .instance_type(e::InstanceType::from(spec.instance_type.as_str()))
    .key_name(spec.key_name.as_str())
    .user_data(&spec.user_data_base64)
    .set_tag_specifications(Some(vec![
        e::LaunchTemplateTagSpecificationRequest::builder()
            .resource_type(e::ResourceType::Instance)
            .set_tags(Some(tags))
            .build(),
    ]));
if !spec.security_group_ids.is_empty() {
    data = data.set_security_group_ids(Some(
        spec.security_group_ids
            .iter()
            .map(|s| s.0.clone())
            .collect(),
    ));
}
```

- [ ] **Step 3: Compile-check**

Run: `cargo check --workspace --all-targets`
Expected: PASS.

- [ ] **Step 4: Run tests**

Run: `cargo nextest run --workspace`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
jj describe -m "chore(cli): make TemplateUpdate --name a flag; skip empty SG list"
jj new
```

---

## Task 10: Final verification

No file edits — verification of the workspace as a whole.

- [ ] **Step 1: Full workspace test run**

Run: `cargo nextest run --workspace`
Expected: all PASS.

- [ ] **Step 2: Clippy clean**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Coverage check (best-effort)**

Run: `cargo llvm-cov nextest --workspace --summary-only`
Expected: line coverage ≥ 50% (the ONBOARDING floor). Note specific changed-line coverage if the tool reports it.

- [ ] **Step 4: Confirm the jj history**

Run: `jj log -r 'main..@' --no-graph -T 'change_id.shortest() ++ " " ++ description.first_line() ++ "\n"'`
Expected: a sequence of revisions ending with the empty working copy (after the last `jj new`), each with a clear Conventional Commit subject:
- `feat(core): add Error::Cancelled variant for SIGINT (exit 130)`
- `feat(core): add wait_or_cancel helper for poll loops`
- `fix(cli): respond to cancel token in probe_ssh_ready`
- `fix(aws): return Cancelled and respond to cancel in instance_wait_running`
- `fix(core): restore raw-input preflight in encode_user_data`
- `fix(core): restore managed-instance gate in Connect::resolve_key`
- `feat(cli): --connect implies bootstrap wait; add --no-wait-bootstrap`
- `refactor(core): async file I/O for user-data reads`
- `chore(cli): make TemplateUpdate --name a flag; skip empty SG list`
- ... plus the empty `@` and the pre-existing `docs: add review-fix design spec` and `(no description set)` for the original feature work.

- [ ] **Step 5: No commit**

This task only verifies; no commit needed.
