# 10 — Development Guidelines

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

This page consolidates the day-to-day rules for working on `kleya`. The spirit is **Tiger-style defensive coding** — small functions, named bounds, paired assertions, no silent fallthroughs — applied to a Rust workspace. CI enforces the mechanical parts (`fmt`, `clippy`, `nextest`, `deny`, `llvm-cov`); humans (and agents) enforce the rest in review.

The full toolchain reference and release process live in [CONTRIBUTING.md](../../CONTRIBUTING.md); this spec captures the principles those tools enforce.

---

## Required tooling

| Tool | Why |
|---|---|
| `rustc 1.95.0` | Pinned in `rust-toolchain.toml` |
| `cargo-nextest` | The only sanctioned test runner |
| `cargo-deny` | License + advisory + version-bans check |
| `cargo-llvm-cov` | Coverage floor (50 %) |
| `cargo-dist` (releases only) | Builds + publishes release artifacts |
| `jj` | Preferred VCS — colocated jj-on-git |
| `lefthook` | Pre-push hook runner |
| `docker` | Optional; required only for the local Floci tier |
| `gh` | Tag pushes + GitHub interactions |

`jj` is preferred but optional — the repo is a colocated jj-on-git checkout, so plain git works too. Agents should use jj.

---

## Tiger-style coding rules

Enforced by clippy + lefthook + review.

- **70-line function cap.** `clippy::too_many_lines` is warned at the workspace level and denied via `-D warnings` in CI / lefthook. A function above 70 lines is split or refactored.
- **100-column line cap.** `rustfmt` formats to 100 columns; `cargo fmt --check` enforces.
- **Two assertions per function minimum** for non-trivial functions: one input precondition, one output postcondition. See [crates/kleya-core/src/bootstrap/render.rs](../../crates/kleya-core/src/bootstrap/render.rs) for the canonical pattern.
- **No `unwrap` / `expect` / `panic!` / `todo!` / `unimplemented!` / `dbg!`** in production paths. The workspace clippy config denies them:

  ```toml
  [workspace.lints.clippy]
  unwrap_used = "deny"
  expect_used = "deny"
  panic = "deny"
  todo = "deny"
  unimplemented = "deny"
  dbg_macro = "deny"
  ```

  `clippy.toml` allows them in test files only (`allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests`). Narrow `#[allow(clippy::expect_used)]` is acceptable for `Lazy<Regex>` static initializers with a comment explaining why the compile-time expression can't fail.
- **`unsafe_code = "forbid"`** at the workspace level. No `unsafe` blocks anywhere.
- **Newtypes at the boundary.** `InstanceId`, `KeyName`, `Region`, `TemplateName`, etc. validate at construction. Domain code receives the validated newtype, not `String`.
- **One `Error` enum per crate** via `thiserror`. Adapter errors wrap into `kleya_core::Error::Adapter { provider, source }` at the public boundary. See [07-error-model.md](07-error-model.md).
- **Idempotent `ensure_default_*` methods** on `CloudCompute`. Adapters treat provider-side "already exists" as success after a follow-up `Describe*` confirms the resource matches. See [04-provider-port.md](04-provider-port.md).

---

## Named limits

**All bounds live in [crates/kleya-core/src/limits.rs](../../crates/kleya-core/src/limits.rs).** Magic numbers anywhere else are a smell.

Current catalogue:

| Constant | Value | Where used |
|---|---|---|
| `CONFIG_BYTES_MAX` | `256 * 1024` (256 KiB) | `config_loader::load` |
| `USER_DATA_RAW_BYTES_MAX` | `16 * 1024` (16 KiB) | `encode_user_data_passthrough`; preflight `× 4` in default path |
| `USER_DATA_GZIP_BYTES_MAX` | `16 * 1024` (16 KiB) | `encode_user_data` post-gzip |
| `USER_DATA_BASE64_BYTES_MAX` | `21_848` | `debug_assert!` ceiling on encoded form |
| `TEMPLATES_COUNT_MAX` | `64` | `Config::validate` |
| `TAGS_PER_TEMPLATE_MAX` | `50` | `Config::validate` |
| `TAG_KEY_BYTES_MAX` | `128` | `Tag::new` |
| `TAG_VALUE_BYTES_MAX` | `256` | `Tag::new` |
| `INSTANCE_NAME_BYTES_MAX` | `63` | `InstanceName::new` |
| `KEY_NAME_BYTES_MAX` | `128` | `KeyName::new` |
| `LAUNCH_WAIT_SECONDS_MAX` | `600` | `Deadline.timeout` |
| `LAUNCH_POLL_INTERVAL_SECONDS` | `5` | `Deadline.poll_interval` |
| `SSH_PROBE_PORT` | `22` | `probe_ssh_ready` |
| `SSH_PROBE_TIMEOUT_SECONDS` | `180` | `probe_ssh_ready` |
| `SSH_PROBE_INTERVAL_SECONDS` | `3` | `probe_ssh_ready` |
| `SSH_PROBE_TCP_TIMEOUT_MS` | `2_000` | `probe_ssh_ready` per-attempt timeout |
| `AWS_CALL_TIMEOUT_SECONDS` | `30` | SDK call timeout (forward-looking) |
| `AWS_RETRY_ATTEMPTS_MAX` | `5` | SDK retry policy (forward-looking) |
| `AWS_RETRY_BACKOFF_BASE_MS` | `200` | SDK retry policy |
| `AWS_RETRY_BACKOFF_CAP_MS` | `5_000` | SDK retry policy |

Compile-time `const _: () = assert!(…)` relationships keep the values consistent — see [09-architecture-principles.md](09-architecture-principles.md#compile-time-invariants). Every limit boundary is tested at `value - 1`, `value`, `value + 1`.

---

## Testing conventions

The full tier breakdown is in [08-testing.md](08-testing.md). The principles:

- Unit tests live in `#[cfg(test)] mod tests` alongside the code they test.
- Integration tests use `InMemoryCompute` + `InMemoryKeyStore` from `kleya_core::test_support` (feature-gated). No mocking the database, no mocking IPC.
- Boundary triple at every named limit.
- Negative space: every limit, every regex, every parser gets at least one positive and one negative test.
- Snapshot tests (via `insta`) for the rendered user-data script. Update with `cargo insta review` only after a deliberate change to the template or its variables.

---

## Telemetry

`tracing` + `tracing-subscriber`. Init in [crates/kleya-cli/src/logging.rs](../../crates/kleya-cli/src/logging.rs):

- Default human-readable, level `info`; `-v` → `debug`, `-vv` → `trace`.
- `--log-format json` switches to a JSON layer. Mandatory structured fields where applicable: `command`, `region`, `template`, `instance_id`.
- Secrets never logged. The SSH argv (containing the key path) is logged at `debug` only; `KLEYA_DEBUG_SECRETS=1` is the documented opt-in for full-argv dumps.
- Named-limit hits emit `tracing::warn!` with structured `limit_hit` fields so operators can grep / alert.

---

## Repository hygiene

- **`.private/` is gitignored.** Nothing operator-specific is committed.
- **jj on Git backend.** `main` is the integration bookmark. Push directly to `main`; PRs are optional but supported.
- **Conventional Commits.** Enforced by lefthook's `commit-msg` hook. Allowed prefixes:
  - `feat:` / `feat(scope):` — new capability
  - `fix:` / `fix(scope):` — bug fix
  - `refactor:` — change shape, not behaviour
  - `test:` — tests only
  - `docs:` — docs only
  - `chore:` — manifest, tooling, deps
  - `ci:` — workflow changes
  - `release:` — version bumps / release prep
  - Body should explain the "why" — the "what" is in the diff.
- **Lefthook pre-push.** Runs `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo nextest run --workspace --no-fail-fast` before allowing a push.

---

## CI gate

`.github/workflows/ci.yml` runs on every push to `main` and on PRs:

| Step | Blocks merge? |
|---|---|
| `cargo fmt --check` | Yes |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Yes |
| `cargo nextest run --workspace` | Yes |
| `cargo deny check` | Yes |
| `cargo llvm-cov --workspace --fail-under-lines 50` | Yes |
| Floci integration test (`floci` job) | **No** (`continue-on-error: true`; see [08-testing.md](08-testing.md)) |
| `skill-package` (build + smoke-test the `using-kleya` skill) | Yes |

All third-party actions are pinned to commit SHAs with a human-readable version comment. When you update an action, look up the new SHA via `gh api repos/<owner>/<repo>/git/refs/tags/<tag> -q '.object.sha'` (deref the tag object for annotated tags).

---

## Definition of done

A change is "done" when:

1. Behaviour is exercised by a test (unit, integration, or e2e), including negative-space tests for every new validation path.
2. Every new / touched non-trivial function has at least two meaningful assertions.
3. Every new bound is a named const in `kleya-core/src/limits.rs`.
4. `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo nextest run --workspace` pass locally.
5. Commit description states the why.
6. PR description (when used) lists architecture-level changes — new ports, new adapters, new surfaces.

---

## Release process

Releases are driven by tag pushes through `cargo-dist`. The workflow is in `.github/workflows/release.yml`, generated by `dist generate --mode=ci` and post-processed to pin actions to commit SHAs.

Cutting a release (abridged; the full sequence is in [CONTRIBUTING.md](../../CONTRIBUTING.md#release-process-cargo-dist)):

```bash
# 1. Bump versions across the workspace
sed -i 's/version = "0.1.0-rc.1"/version = "0.1.0"/' crates/*/Cargo.toml
cargo update --workspace

# 2. Verify dist plans the right matrix
dist plan

# 3. Commit + push the bump
jj describe -m "release: v0.1.0"
jj bookmark move main --to @-
jj git push --bookmark main

# 4. After CI is green, tag (annotated) and push the tag
git tag -a v0.1.0 main -m "v0.1.0"
git push origin v0.1.0          # triggers release.yml
```

The release workflow produces per-target `tar.xz` archives, an installer (`kleya-cli-installer.sh`), a source archive, and a `sha256.sum` aggregate. The release flag is auto-set to "prerelease" if the version has a `-rc`/`-alpha`/`-beta` suffix.

**Important.** Re-running `dist generate --mode=ci` regenerates `release.yml` with unpinned action refs. The `allow-dirty = ["ci"]` setting in `[workspace.metadata.dist]` lets `dist host` tolerate this, but the re-pin of SHAs after every `dist generate` is non-optional.

---

## Dependencies

The workspace dependency catalogue in `Cargo.toml` is the source of truth — adding a dependency means adding it there once and referencing it via `workspace = true` in the per-crate `Cargo.toml`.

`cargo-deny` enforces:

- License allowlist: `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-{2,3}-Clause`, `ISC`, `Unicode-DFS-2016`, `Unicode-3.0`, `MPL-2.0`.
- `yanked = "deny"` — yanked crates fail CI.
- `multiple-versions = "warn"` — diamond dependencies are flagged but not blocking.
- `unknown-registry = "deny"`, `unknown-git = "deny"` — only crates.io and explicitly-listed git sources are allowed.
- Targeted advisory ignores for transitive `rustls-webpki` advisories (`RUSTSEC-2026-0098 / 0099 / 0104`) that upstream has not patched yet. Documented in `deny.toml`; remove when `cargo update` resolves them.

---

## AI-agent rules

Agents working on `kleya` follow the same rules above plus:

- Read this spec set and [CONTRIBUTING.md](../../CONTRIBUTING.md) before non-trivial changes.
- Prefer the `using-kleya` skill (`skills/using-kleya/SKILL.md`) over raw `aws ec2 …` commands when launching test boxes.
- Never use git commands with the `-i` flag (interactive mode is unsupported).
- Never bypass `lefthook` (`--no-verify`) or unwarrantedly `git push --force`.
- When a clippy lint blocks progress, fix the underlying code rather than reaching for `#[allow(…)]` — narrow allows are acceptable only for genuinely unavoidable cases (e.g., static `Lazy<Regex>` initializers).

---

## Assumptions and open questions

**Assumptions**

- The maintainer's environment has `rustc 1.95.0` installed via `rustup`. The pinned toolchain in `rust-toolchain.toml` does the rest.
- `cargo-nextest`, `cargo-deny`, `cargo-llvm-cov` are installed locally for CI parity. The pre-push hook only runs fmt + clippy + nextest; the full gate is opt-in locally.
- `lefthook` runs pre-push, not pre-commit. Conventional-commit enforcement lives in the commit-msg hook; agents using `jj describe` skip git's commit-msg hook by default, so the lint runs on push.

**Decisions**

- *Tiger-style is enforced by lint + review, not by a dedicated linter.* **`too_many_lines` + named-limit grep + two-assertion expectation cover ~90 % of the rules.** A dedicated linter (e.g., `tigerbeetle/style.toml` analogue) is not worth maintaining for a four-crate workspace.
- *Workspace `clippy::pedantic = "warn"`.* **Pedantic is on, exceptions are local `#[allow(...)]` with a comment.** This catches more than `clippy::all` without forcing every clippy quirk to be a build break.
- *Conventional Commits via commit-msg hook.* **Commit messages are searchable; release notes are mechanically generated when needed.** The minor lock-in is fine; the discipline is the win.
- *Coverage floor is 50 %, not 70 % or 80 %.* **A floor that fails CI when meaningfully dropped, not a target to game.** Anything above the floor is good; anything below is a regression.
- *Workspace dependency catalogue.* **One version of every dep across the workspace.** Per-crate version pinning would let diamond-dependency drift creep in.

**Open questions**

- *Pre-commit vs pre-push hook split.* Resolved: leave hooks at pre-push. Splitting fmt + commit-msg to pre-commit and clippy + nextest to pre-push would complicate the jj workflow without a real win.
- *Coverage exclusions.* Deferred: `cargo-llvm-cov` still includes the adapter mapping modules (mostly type-glue). Revisit only if the coverage floor climbs into territory where the mapping modules drag it down.
