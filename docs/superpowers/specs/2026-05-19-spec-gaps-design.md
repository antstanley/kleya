# `kleya` — Spec-Gap Design

- **Date:** 2026-05-19
- **Status:** Draft, ready for plan
- **Scope:** Close the gaps between the v0.1.0 implementation at `797b076` (`release: v0.1.0-rc.2`) and the two existing design specs (`2026-05-16-kleya-bootstrap-design.md`, `2026-05-18-review-fixes-design.md`). Audit found four substantive misses; this spec describes the fixes.

## 1. Purpose

The bootstrap and review-fix specs were almost fully landed by the v0.1.0-rc.2 cut. Initial audit (specs §-by-§ vs source) flagged four items, but a parallel commit (`557b9000 docs: add canonical spec set under docs/specs/`) landed on `main` while this spec was in flight and supersedes one of them. The reconciled gap list:

1. The §9 `KeyOrphaned` recovery path is not operator-actionable — there is no `--regenerate-key` flag on `kleya launch`, so the documented manual override is impossible. Canonical 07-error-model.md still describes the operator decision but does not contradict the draft's flag-based mechanism.
2. §12 lists Floci-tier tests covering `template_lifecycle.rs` **and** `instance_lifecycle.rs`. Only the former exists; canonical 08-testing.md §25 keeps the same expectation ("instance launch/list/terminate happy paths + induced error per code path").
3. §7 demands a negative-space test: padding `extra_post_lines` past the operative limit must return `Error::UserDataTooLarge`. The unit test in `encode.rs` covers the encoder directly but no integration test exercises the full `render → encode` path past the limit; canonical 05-bootstrap-rendering.md §106-110 preserves the two-stage size check.

**Dropped after canonical-spec reconciliation:**

- *Structured telemetry fields (`command`, `region`, `template`, `instance_id`).* The 2026-05-16 draft §13 listed these as "mandatory" under `--log-format json`, but the canonical 00-overview.md §104 and 02-cli-surface.md §142-143 describe JSON mode as "switches the formatter only" with no mandatory fields. Reading both specs together, the canonical wording is the operative requirement — and it is met by the current `logging.rs`. The draft-only requirement is dropped here as out of scope; a future change can re-introduce structured spans if operators ask for them.

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

### 2.2 Floci `instance_lifecycle.rs`

**Why:** §12's Floci row lists "template create/list/delete, instance launch/list/terminate happy path + induced error per code path". Today the file `template_lifecycle.rs` exists and is `#[ignore]`-gated on `KLEYA_TEST_FLOCI=1`; the matching `instance_lifecycle.rs` is missing.

**Scope:**

- `crates/kleya-aws/tests/floci/instance_lifecycle.rs` — new. Three `#[tokio::test]` `#[ignore]` cases:
  - `instance_launch_then_list_then_terminate` — happy path. Use a previously-created template (call `ensure_default_template` in the fixture); `instance_launch` with a fixed name; `instance_list` filters down to one; `instance_terminate`; `instance_list` filter yields zero. Assertions: at least two per test, including round-trip on `tag:kleya:managed=true` filter.
  - `instance_terminate_on_unknown_id_is_adapter_error` — induced error; pass `InstanceId::new("i-deadbeef").unwrap()`; expect `Error::Adapter`.
  - `instance_list_with_empty_filter_returns_all_managed` — boundary; precondition is one launched instance; assert the returned set contains that id.
- `crates/kleya-aws/tests/floci/mod.rs` — add `pub mod instance_lifecycle;`.
- The Floci docker image and `OnceCell` start/stop guard already exist in `mod.rs`; no harness change.

**Note on Floci capability:** today's CI marks the Floci job `continue-on-error: true` because Floci lacks `CreateLaunchTemplate` (see `.github/workflows/ci.yml`). The new tests degrade gracefully: when `CreateLaunchTemplate` fails, the test prints "Floci does not support this op, skipping" and returns early with `Ok(())` so the run is green even on incompatible Floci versions. Once Floci adds support, the tests start asserting for real with no code change.

### 2.3 `UserDataTooLarge` integration test

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
