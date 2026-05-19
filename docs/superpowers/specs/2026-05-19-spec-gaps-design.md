# `kleya` — Spec-Gap Design

- **Date:** 2026-05-19
- **Status:** Draft, ready for plan
- **Scope:** Close the gaps between the v0.1.0 implementation at `797b076` (`release: v0.1.0-rc.2`) and the two existing design specs (`2026-05-16-kleya-bootstrap-design.md`, `2026-05-18-review-fixes-design.md`). Audit found four substantive misses; this spec describes the fixes.

## 1. Purpose

The bootstrap and review-fix specs were almost fully landed by the v0.1.0-rc.2 cut, but a parallel audit (specs §-by-§ vs source) surfaced four items the implementation does not satisfy:

1. The §9 `KeyOrphaned` recovery path is not operator-actionable — there is no `--regenerate-key` flag on `kleya launch`, so the documented manual override is impossible.
2. §13 mandates structured telemetry fields (`command`, `region`, `template`, `instance_id`) on `--log-format json`. Today the JSON layer is wired but the fields are not attached at the command boundary; `grep -n` finds two stray `tracing::info!` calls in `launch.rs` only.
3. §12 lists Floci-tier tests covering `template_lifecycle.rs` **and** `instance_lifecycle.rs`. Only the former exists.
4. §7 demands a negative-space test: padding `extra_post_lines` past the operative gzip limit must return `Error::UserDataTooLarge`. The unit test in `encode.rs` covers the encoder directly but no integration test exercises the full `render → encode` path past the limit.

Two further items in the bootstrap-spec audit are explicitly **non-gaps** and out of scope here:

- **Per-subcommand `--region`** on `template create`/`update` — Spec §4 lists it on every subcommand, but a `global = true` clap arg already covers every dispatch site uniformly. Re-declaring it per-subcommand would be cosmetic; the resolved-region path is unchanged.
- All eight items in the 2026-05-18 review-fix spec are present (audit confirmed `Error::Cancelled`, `wait_or_cancel` helper, encode preflight, managed-instance gate, `--no-wait-bootstrap`, async file I/O, `TemplateUpdateArgs.name`, empty-SG guard).

## 2. Changes

### 2.1 `--regenerate-key` flag and orphan-recovery path

**Why:** When EC2 has a key registered under a name but the local pem is gone (laptop wiped, dotfiles reset), the current code throws `Error::KeyOrphaned` and stops. Bootstrap spec §9 explicitly defers this to "operator decides (`--regenerate-key` flag)". The flag is the only mechanism that lets an operator recover without dropping to the AWS console.

**Surface:**

```
kleya launch ... [--regenerate-key]
```

`LaunchArgs` gains:

```rust
/// On (local-absent, EC2-present): delete the EC2 key, generate a fresh
/// local Ed25519 pair, and re-register the public half.
#[arg(long)]
pub regenerate_key: bool,
```

`LaunchPlan` / `LaunchService::launch` plumbs the bool through; `LaunchService::ensure_keypair` gains a `regenerate: bool` parameter (call sites pass `args.regenerate_key`).

**Lifecycle table** (revised; new row in **bold**):

| Local key | EC2 key | `--regenerate-key` | Action |
|---|---|---|---|
| present | present (fp match) | (any) | use as-is |
| present | present (fp differ) | (any) | `Error::KeyMismatch` |
| present | absent | (any) | `ImportKeyPair` |
| absent  | present | false | `Error::KeyOrphaned` (status quo) |
| **absent**  | **present** | **true**  | **delete EC2 key → generate → `ImportKeyPair`** |
| absent  | absent  | (any) | generate → `ImportKeyPair` |

**Provider port:** `CloudCompute` already has `instance_terminate` and `template_delete` but no `keypair_delete`. Add:

```rust
async fn keypair_delete(&self, name: &KeyName) -> Result<()>;
```

`InMemoryCloudCompute` removes the entry from its map; `AwsEc2` calls `DeleteKeyPair` and treats `InvalidKeyPair.NotFound` as success (idempotent, matches the existing ensure-pattern). The follow-up `DescribeKeyPairs` confirm round-trip established by review-fix §2.x for `ensure_default_keypair` is mirrored here: after `DeleteKeyPair`, issue one `DescribeKeyPairs(key_names=[name])` and treat presence as `Error::Adapter { reason: "delete acknowledged but key still listed" }` — guards the same TOCTOU window.

**Tests:**

- `kleya-core/src/commands/launch.rs`:
  - `ensure_keypair_regenerates_on_orphan_when_flag_set` — fake compute reports the name registered, fake key store has no local file, `regenerate = true` ⇒ store now has the file AND compute records the **new** public half.
  - `ensure_keypair_still_errors_on_orphan_when_flag_unset` — same setup, `regenerate = false` ⇒ `Error::KeyOrphaned`.
- `kleya-core/src/test_support/in_memory_compute.rs`: add `keypair_delete` impl (idempotent on absence) — covered transitively by the new launch test.
- `kleya-cli/tests/cli_smoke.rs` or `cli_dispatch_args.rs`: assert `kleya launch --regenerate-key --dry-run` parses without error and that the bool reaches `LaunchPlan`.
- `kleya-aws/tests/floci/keypair_lifecycle.rs` — new file, `#[ignore]` gated on `KLEYA_TEST_FLOCI=1`; round-trips `ensure_default_keypair → keypair_delete → ensure_default_keypair`, ensures fingerprint of the second registration differs from the first.

**Error mapping:** no new variants. Delete-failure surfaces as `Error::Adapter { provider: "aws-ec2", source }`, exit code 70.

### 2.2 Structured telemetry fields (`command`, `region`, `template`, `instance_id`)

**Why:** Spec §13 lists these four as mandatory fields under `--log-format json`. They are not currently emitted, which makes the JSON log mode less useful than the human-readable mode — operators cannot grep for `instance_id=i-…` because no event carries the field.

**Approach:** Use `tracing::Span` at the command boundary, not per-event field plumbing. Each top-level dispatch arm in `kleya-cli/src/dispatch.rs` opens an `info_span!` with the available fields and enters it for the duration of the call. Sub-events (`info!`, `warn!`) inherit the span fields automatically under the JSON layer's `flatten_event` formatting.

**Field availability** by command:

| Command   | `command` | `region` | `template` | `instance_id` |
|-----------|-----------|----------|------------|---------------|
| launch    | "launch"  | yes      | yes        | filled in after `instance_launch` returns (via `Span::current().record`) |
| connect   | "connect" | yes      | tag-derived if present | yes (after `resolve_handle`) |
| terminate | "terminate" | yes    | tag-derived | yes |
| list      | "list"    | yes      | — | — |
| template create/update/list/delete | "template:<verb>" | yes | yes (name) | — |
| config show/path | "config:<verb>" | — | — | — |

A field that doesn't apply for a given command is **omitted from the span declaration**, not emitted as `null`. JSON consumers see only present fields, matching the spec's "where applicable".

**Implementation sketch:**

```rust
// In each dispatch arm:
let span = tracing::info_span!(
    "kleya",
    command = "launch",
    region = %resolved_region,
    template = %template_name,
    instance_id = tracing::field::Empty,
);
let _enter = span.enter();
// ... later, once we have the id:
tracing::Span::current().record("instance_id", &tracing::field::display(id.as_str()));
```

The text formatter strips span fields by default, so the human-readable output is unchanged.

**Tests:**

- `kleya-cli/tests/logging_fields.rs` (new). Capture tracing output via a custom `tracing_subscriber::layer::Layer` that records events into a `Vec<Value>`. Drive `dispatch::run_with` for `list` (smallest payload) and for `launch --dry-run` against fakes; assert the captured events include `command`, `region`, and (for `launch --dry-run`) **no** `instance_id` field (dry-run never launches).
- The Floci tier is unaffected: spans don't change behaviour, only output.

**Non-goals (spec §13):** the `KLEYA_DEBUG_SECRETS=1` argv dump and the `limit_hit` warn counter already exist (the latter is implicit in `Error::UserDataTooLarge` formatting; an explicit counter is out of scope and tracked separately).

### 2.3 Floci `instance_lifecycle.rs`

**Why:** §12's Floci row lists "template create/list/delete, instance launch/list/terminate happy path + induced error per code path". Today the file `template_lifecycle.rs` exists and is `#[ignore]`-gated on `KLEYA_TEST_FLOCI=1`; the matching `instance_lifecycle.rs` is missing.

**Scope:**

- `crates/kleya-aws/tests/floci/instance_lifecycle.rs` — new. Three `#[tokio::test]` `#[ignore]` cases:
  - `instance_launch_then_list_then_terminate` — happy path. Use a previously-created template (call `ensure_default_template` in the fixture); `instance_launch` with a fixed name; `instance_list` filters down to one; `instance_terminate`; `instance_list` filter yields zero. Assertions: at least two per test, including round-trip on `tag:kleya:managed=true` filter.
  - `instance_terminate_on_unknown_id_is_adapter_error` — induced error; pass `InstanceId::new("i-deadbeef").unwrap()`; expect `Error::Adapter`.
  - `instance_list_with_empty_filter_returns_all_managed` — boundary; precondition is one launched instance; assert the returned set contains that id.
- `crates/kleya-aws/tests/floci/mod.rs` — add `pub mod instance_lifecycle;`.
- The Floci docker image and `OnceCell` start/stop guard already exist in `mod.rs`; no harness change.

**Note on Floci capability:** today's CI marks the Floci job `continue-on-error: true` because Floci lacks `CreateLaunchTemplate` (see `.github/workflows/ci.yml`). The new tests degrade gracefully: when `CreateLaunchTemplate` fails, the test prints "Floci does not support this op, skipping" and returns early with `Ok(())` so the run is green even on incompatible Floci versions. Once Floci adds support, the tests start asserting for real with no code change.

### 2.4 `UserDataTooLarge` integration test

**Why:** §7 specifies: "padding `extra_post_lines` past the operative limit must return `Error::UserDataTooLarge { bytes, max }`". The encode-layer unit test covers the gzip-output cap; what's missing is an end-to-end `render → encode` test confirming the full pipeline propagates the error.

**Scope:** Add to `crates/kleya-core/tests/render_snapshot.rs` (or new `tests/bootstrap_limits.rs`):

```rust
#[test]
fn render_then_encode_rejects_oversize_extra_post_lines() {
    let mut extras = Vec::new();
    // 60 KiB of padding — well past every cap, render output >> RAW max
    let line = "echo padding".repeat(64);                  // ~768 B per line
    for _ in 0..96 { extras.push(line.clone()); }          // ~72 KiB
    let vars = BootstrapVars {
        install_ghostty_terminfo: false,
        ghostty_terminfo_source: "",
        install_dev_tools: true,
        node_major: 22,
        python_version: "3.12",
        extra_pre_lines: &[],
        extra_post_lines: &extras,
    };
    let rendered = render(&vars).expect("render");
    let err = encode_user_data(&rendered).expect_err("must reject");
    assert!(
        matches!(err, kleya_core::Error::UserDataTooLarge { .. }),
        "got: {err:?}",
    );
}
```

Two assertions per the Tiger-style requirement: matches-`UserDataTooLarge` and the `extras` length precondition (`assert!(extras.iter().map(String::len).sum::<usize>() > USER_DATA_RAW_BYTES_MAX * 4);` at the top).

## 3. Out of scope

- Per-subcommand `--region` (cosmetic; see §1).
- Migrating `once_cell::sync::Lazy` → `std::sync::LazyLock` (still deferred from 2026-05-18 §4).
- Any change to `ensure_keypair` semantics on the non-orphan rows.
- `limit_hit` warn counter from §13 — separate change.

## 4. Acceptance

- `cargo nextest run --workspace` passes (incl. four new tests).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo llvm-cov` line floor (50%) holds.
- Manual: `kleya launch --regenerate-key --dry-run` parses and prints a plan that mentions key regeneration; `kleya launch --log-format json --dry-run` produces JSON events containing `command="launch"` and `region="…"`.
- Floci tests under `KLEYA_TEST_FLOCI=1` either pass or skip cleanly with a printed reason.
