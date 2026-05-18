# `kleya` — Review-Fix Design Spec

- **Date:** 2026-05-18
- **Status:** Draft, ready for plan
- **Scope:** Resolve the six concerns and two minor items raised in the semi-formal review of the pending branch (SSH probe + cancellation + EC2 adapter refactor). Out of scope: `ensure_keypair` short-circuit, `describe_key_pairs` confirm round-trip, `once_cell` → `LazyLock` migration.

## 1. Purpose

The pending branch wires up the `launch --connect`/`--wait-bootstrap` flow, a `CancellationToken` plumbed through `Deadline`, and a structural refactor of the EC2 adapter. The review found six semantic concerns and two minor surface inconsistencies. This spec specifies the fixes; an implementation plan follows.

## 2. Changes

### 2.1 New `Error::Cancelled` variant

`crates/kleya-core/src/error.rs`

```rust
#[error("cancelled: {instance}")]
Cancelled { instance: InstanceId },
```

- Map to **exit code 130** (= 128 + SIGINT) in `crates/kleya-cli/src/exit_code.rs`.
- `Error::LaunchWaitTimeout` keeps its current meaning: wall-clock or attempt-count exhaustion.

### 2.2 Responsive cancellation in poll loops

Two loops poll on an interval and must observe the cancel token without waiting for the interval to elapse:

- `crates/kleya-aws/src/ec2.rs::AwsEc2::instance_wait_running`
- `crates/kleya-cli/src/ssh_probe.rs::probe_ssh_ready`

Replace bare `tokio::time::sleep(interval).await` with:

```rust
match &cancel {
    Some(c) => tokio::select! {
        () = c.cancelled() => return Err(Error::Cancelled { instance: id.clone() }),
        () = tokio::time::sleep(interval) => {}
    },
    None => tokio::time::sleep(interval).await,
}
```

Drop the now-redundant top-of-loop `is_cancelled()` check in `instance_wait_running`.

`probe_ssh_ready` gains a `cancel: Option<CancellationToken>` parameter. Call sites in `dispatch.rs::Cmd::Launch` and `Cmd::Connect` pass `Some(cancel.clone())`.

### 2.3 Restore `encode_user_data` preflight

`crates/kleya-core/src/bootstrap/encode.rs`

Restore the cheap raw-size short-circuit before gzip allocation:

```rust
if raw.len() > USER_DATA_RAW_BYTES_MAX * 4 {
    return Err(Error::UserDataTooLarge {
        bytes: raw.len(),
        max: USER_DATA_RAW_BYTES_MAX * 4,
    });
}
```

Re-add the deleted `rejects_oversize_raw_input` test. `encode_user_data_passthrough` is unchanged (its 16 KiB cap is correct).

### 2.4 Restore `Connect::resolve_key` managed gate

`crates/kleya-core/src/commands/connect.rs`

When an instance has no `kleya:key` tag, restore the `kleya:managed=true` precondition before falling back to `config.keys.default_key_name`. Reintroduce the `KLEYA_TAG_MANAGED` import:

```rust
let managed = inst.tags.iter().any(|t| t.key == KLEYA_TAG_MANAGED && t.value == "true");
if !managed {
    return Err(Error::ConfigInvalid {
        reason: format!(
            "instance {} not managed by kleya; pass --instance-id and configure a key",
            inst.id.as_str()
        ),
    });
}
KeyName::new(self.config.keys.default_key_name.clone())
```

The empty-`default_key_name` check stays as a secondary guard.

### 2.5 `--connect` implies cloud-init wait

`crates/kleya-cli/src/clap_args.rs`, `crates/kleya-cli/src/dispatch.rs`

Add a `--no-wait-bootstrap` flag on `LaunchArgs`. Effective rule in `dispatch.rs::Cmd::Launch`:

```rust
let wait_bootstrap = args.wait_bootstrap || (args.connect && !args.no_wait_bootstrap);
```

`args.wait_bootstrap` (the existing flag) stays — users running `launch --wait-bootstrap` without `--connect` keep current behavior. `--no-wait-bootstrap` only meaningfully overrides when `--connect` is also set.

### 2.6 Async file I/O

`crates/kleya-cli/src/dispatch.rs::build_template_spec` and
`crates/kleya-core/src/commands/launch.rs::LaunchService::render_user_data`

Switch `std::fs::read` → `tokio::fs::read` and make both functions `async`. All call sites are already in `async fn`s. `build_template_spec` is moved to `async`; the three `Cmd::Template::Create/Update` arms `.await` it.

### 2.7 `TemplateUpdateArgs.name` consistency

`crates/kleya-cli/src/clap_args.rs`

Add `#[arg(long)]` to `TemplateUpdateArgs.name` so the surface matches `TemplateCreateArgs`. Both become `kleya template create --name foo …` and `kleya template update --name foo …`.

### 2.8 Skip empty security-group list

`crates/kleya-aws/src/ec2.rs::build_request_launch_template_data`

```rust
if !spec.security_group_ids.is_empty() {
    data = data.set_security_group_ids(Some(
        spec.security_group_ids.iter().map(|s| s.0.clone()).collect(),
    ));
}
```

Avoids passing `Some(vec![])` to the SDK; lets AWS apply the VPC default SG cleanly.

## 3. Test deltas

- `crates/kleya-core/src/error.rs`: add `cancelled_display_contains_instance` (asserts `format!("{e}")` includes the instance id).
- `crates/kleya-cli/src/exit_code.rs`: add `cancelled_maps_to_130`.
- `crates/kleya-core/src/bootstrap/encode.rs`: restore `rejects_oversize_raw_input` (raw > 4×RAW_MAX).
- `crates/kleya-core/src/commands/connect.rs`: add `resolve_key_rejects_unmanaged_instance`.
- `crates/kleya-cli/src/ssh_probe.rs`: add a cancellation-responsive unit test (probe a port that refuses, cancel mid-loop, expect `Error::Cancelled` within < interval).
- `crates/kleya-core/tests/commands_with_fakes.rs`: extend an existing launch-with-connect test or add one asserting the effective `wait_bootstrap = wait || (connect && !no_wait)` rule at the dispatch level (will be covered in `kleya-cli` integration tests rather than core fakes, since the rule lives in dispatch).

Snapshot tests for the bootstrap script are unaffected — no template changes.

## 4. Out of scope

- `ensure_keypair` always-runs vs. old short-circuit. The new behavior is strictly more defensive; the one extra `keypair_fingerprint` call is negligible against the launch wall clock.
- `describe_key_pairs` confirm round-trip after `ensure_default_keypair`. Cheap insurance against narrow TOCTOU; not worth removing.
- `once_cell::sync::Lazy` → `std::sync::LazyLock`. Pure style; better as a separate workspace-wide chore commit.

## 5. Acceptance

- `cargo nextest run --workspace` passes.
- `cargo llvm-cov` line coverage stays at or above the current floor (50%).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Manual cancellation: `kleya launch` with Ctrl-C surfaces `Error::Cancelled`, exit code 130, within the TCP probe timeout (2 s).
- Manual `--connect` without `--wait-bootstrap` no longer ssh's in mid-bootstrap; with `--no-wait-bootstrap` it does.
