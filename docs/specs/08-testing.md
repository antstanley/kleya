# 08 — Testing

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`cargo-nextest` is the only sanctioned test runner. Tests are tiered: unit and integration always run; the AWS-shaped Floci tier is `#[ignore]`d behind an env var; the live-AWS tier exists in principle and runs against a sandbox account on PR-to-main when enabled.

---

## Responsibilities

1. Exercise every public behaviour with a deterministic test — pure rendering, validation, key fingerprinting, handle resolution, exit codes.
2. Verify boundary behaviour at named limits (`value - 1`, `value`, `value + 1`).
3. Snapshot the rendered user-data script so template edits are deliberate.
4. Property-test the multi-format config loader for round-trip equality.
5. Run the AWS adapter against a local EC2 emulator (Floci) for shape-level verification of the SDK mapping.

---

## Tiers

| Tier | Scope | Location | Gating |
|---|---|---|---|
| Unit (sub-second) | `bootstrap::render` / `encode`, `Config::validate`, `build_argv`, `resolve_handle`, `Fingerprint` compare, regex validation, limit boundary checks | `#[cfg(test)] mod tests` in each module | Always-on |
| Integration (seconds) | Full subcommands dispatched against `InMemoryCompute` + `InMemoryKeyStore`; deterministic via `FakeClock` + `FakeIdGen`. Snapshot tests on rendered user-data. | `crates/kleya-core/tests/`, `crates/kleya-cli/tests/` | Always-on |
| AWS-shaped (Floci) | `kleya-aws` adapter against a Floci EC2 emulator — template create/list/delete, instance launch/list/terminate happy paths + induced error per code path | `crates/kleya-aws/tests/floci/` | `#[ignore]` unless `KLEYA_TEST_FLOCI=1` |
| e2e (real AWS, slow) | Same harness as the Floci tier but pointed at a sandbox AWS account | `crates/kleya-aws/tests/e2e/` (when enabled) | `#[ignore]` unless `KLEYA_TEST_E2E=1`; run on PR-to-main only |

The tiered split keeps the developer inner loop fast — `cargo nextest run --workspace` is the unit + integration matrix, and runs in well under a minute. Floci is opt-in via env var; live AWS is opt-in via env var and a sandbox profile.

---

## Per-crate test conventions

### Unit tests

- Live in `#[cfg(test)] mod tests` alongside the code they test.
- Use `cargo nextest run -p <crate>` for a single-crate run. `kleya-core` requires `--features test-support` because the in-memory fakes are feature-gated.
- Every named limit gets a boundary triple: `value - 1` (passes), `value` (passes at the cap), `value + 1` (fails). See [model/instance.rs::name_rejects_at_and_above_size_limit](../../crates/kleya-core/src/model/instance.rs) for the canonical pattern.
- Every regex gets at least one positive and one negative case.
- Every newtype constructor has a test for path-traversal-ish inputs where applicable (`KeyName::rejects_leading_dot`, `rejects_path_traversal_chars`).

### Integration tests

- `crates/kleya-core/tests/commands_with_fakes.rs` exercises `LaunchService::run`, `ConnectService::plan`, `TerminateService` against `InMemoryCompute` + `InMemoryKeyStore`.
- `crates/kleya-core/tests/render_snapshot.rs` snapshots the rendered user-data scripts (`setup_devbox_default`, `setup_devbox_no_ghostty`).
- `crates/kleya-cli/tests/`:
  - `cli_smoke.rs` — runs `kleya --help`, `kleya config show` via `assert_cmd`.
  - `config_roundtrip.rs` — multi-format property tests for `Config`.
  - `key_store_fs.rs` — directory mode, file mode, fingerprint stability, regenerate-on-delete.

### Floci-backed adapter tests

[crates/kleya-aws/tests/floci/](../../crates/kleya-aws/tests/floci/) — `mod.rs` plus a `template_lifecycle.rs` test. Gated:

```
KLEYA_TEST_FLOCI=1 KLEYA_TEST_FLOCI_ENDPOINT=http://localhost:4566 \
AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test AWS_SESSION_TOKEN=test \
AWS_REGION=eu-west-1 \
  cargo nextest run -p kleya-aws --run-ignored all
```

The adapter's `client::build_ec2_client(region, endpoint_url)` accepts a `Some("http://localhost:4566")` to point at the emulator; production passes `None`. This keeps the emulator override inside `kleya-aws` — `kleya-core` never knows the adapter is talking to an emulator.

### Floci known limitation

Floci does not implement `CreateLaunchTemplate` (returns `UnsupportedOperation`). The CI `floci` job runs with `continue-on-error: true` so it does not block the workflow; the gate (drop `continue-on-error`) re-enables once Floci ships the op or the test is rewritten against the supported subset. Documented in [CONTRIBUTING.md](../../CONTRIBUTING.md#floci-integration-tests-optional).

---

## Property tests

`proptest` is used at exactly two sites:

1. **Config round-trip.** Generate a valid `Config`; export through TOML / YAML / JSON / JSONC; reload via `parse_by_ext`; assert equality. Catches divergence between the four parsers and any silent serde quirk that would split a config in half between formats. See [03-configuration.md](03-configuration.md).
2. **Handle resolution state.** Generate random instance fleets and tag combinations; assert `resolve(opts)` always returns exactly one of `InstanceNotFound`, `Instance`, or `AmbiguousHandle` with non-empty candidates.

The proptest workspace dependency is pinned at `proptest = "1"`. Test cases use `prop_compose!` for the `Config` generator; shrinking is enabled.

---

## Snapshot tests (`insta`)

Stored at [crates/kleya-core/tests/snapshots/](../../crates/kleya-core/tests/snapshots/):

- `render_snapshot__setup_devbox_default.snap` — full rendered script with ghostty terminfo on and dev-tools off (the production launch path's default).
- `render_snapshot__setup_devbox_no_ghostty.snap` — same script with `install_ghostty_terminfo = false`.

Snapshots are reviewed with `cargo insta review`; never regenerate blindly. A snapshot diff in CI means either the template changed (intentional → review and commit the new `.snap`) or something else changed the rendering (unintentional → bug).

---

## Coverage

`cargo llvm-cov --workspace --fail-under-lines 50` runs in CI. The 50 % line floor is a guardrail, not a target — agent contributors are expected to add tests for the code they write, not pad against the floor. Adapter mapping modules (mostly type-glue) are excluded.

---

## CI matrix

[.github/workflows/ci.yml](../../.github/workflows/ci.yml):

| Job | Steps | Blocking? |
|---|---|---|
| `test` | `fmt --check`, `clippy -D warnings`, `nextest run --workspace`, `deny check`, `llvm-cov --fail-under-lines 50` | Yes |
| `floci` | Wait for Floci health, run `cargo nextest run -p kleya-aws --run-ignored all` against `localhost:4566` | **No** (`continue-on-error: true`) |
| `skill-package` | Package the `using-kleya` skill, verify the `.skill` archive, smoke-test `install-skill.sh` against a fake `$HOME` | Yes |

All third-party actions are pinned to commit SHAs with a human-readable version comment. When you update an action, look up the new SHA via `gh api repos/<owner>/<repo>/git/refs/tags/<tag> -q '.object.sha'`.

---

## Local commands

```bash
# Whole workspace (what CI runs)
cargo nextest run --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo deny check
cargo llvm-cov --workspace --fail-under-lines 50

# A single test
cargo nextest run -p kleya-core --lib error::tests::cancelled_display_contains_instance

# kleya-core needs the test-support feature for per-crate runs:
cargo nextest run -p kleya-core --features test-support

# Floci-backed adapter tests (Docker required)
docker run -d --rm --name kleya-floci -p 4566:4566 \
  floci/floci@sha256:43f48b8cd04354f356b859cc43a8915a88516df6530d4691159bed39b7e9ea32
KLEYA_TEST_FLOCI=1 KLEYA_TEST_FLOCI_ENDPOINT=http://localhost:4566 \
AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test AWS_REGION=eu-west-1 \
  cargo nextest run -p kleya-aws --run-ignored all
```

`lefthook install` wires the pre-push hook that runs `cargo fmt --check`, `cargo clippy …`, and `cargo nextest run` before allowing a push.

---

## Assumptions and open questions

**Assumptions**

- `cargo nextest` is installed (`cargo install cargo-nextest --locked`). The workspace does not vendor a fallback to plain `cargo test`.
- `cargo-deny` and `cargo-llvm-cov` are installed for CI parity locally. The pre-push hook only runs fmt + clippy + nextest; the full CI gate is opt-in locally.
- Docker is available when running the Floci tier. The CI job uses a GitHub Actions service container; local runs use plain `docker run`.
- `insta` snapshot reviews happen during human review of PRs that touch the template. No agent should regenerate snapshots without explicit human approval of the diff.

**Decisions**

- *`cargo-nextest`, not `cargo test`.* **Parallelism, retries, failure summaries.** The cost is a one-time install per environment; the gain on a workspace this size is small in absolute terms but the failure-clarity benefit is large.
- *50 % coverage floor.* **A floor, not a target.** Coverage above 50 % is good; coverage below 50 % is a regression that fails CI. Higher floors invite test-padding behaviour we don't want.
- *Boundary triples for every named limit.* **Off-by-one errors at limits are the most common bug class in this kind of code.** The triple is so cheap to write that there's no reason not to.
- *Snapshot the rendered user-data, not the template.* **The template is a static asset; the rendering is what runs on production boxes.** A template change without a snapshot diff is a regression risk.
- *Floci as a CI smoke check, not a gate.* **Until Floci supports `CreateLaunchTemplate`, the only end-to-end signal we can get from it is `UnsupportedOperation`.** Keeping the job in CI as `continue-on-error: true` means we notice when Floci ships the op (the job goes green); making it blocking would be self-defeating.

**Open questions**

- *Mutation testing.* Resolved: add `cargo-mutants` now, targeting the validator and handle-resolution paths. Even though the workspace is small, the surface that matters most for correctness is bounded and worth mutating.
- *e2e against real AWS in CI.* Resolved: a sandbox AWS account will be provisioned. Document the IAM permissions and AWS configuration the tests require, and ensure every test cleans up the resources it creates.
- *Floci `ec2:RunInstances` coverage.* Deferred (upstream): blocked on Floci shipping `CreateLaunchTemplate`. Expanding the Floci tier from template-only to launch-and-terminate is out of scope for kleya until then.
