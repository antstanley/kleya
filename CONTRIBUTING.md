# Contributing to kleya

This guide is written for both humans and coding agents. It covers the repo layout, required tooling, coding conventions, the test/CI gate, and the release process. The full design rationale lives in [`docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md`](docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md); read that before making non-trivial changes.

## Repo layout

```
kleya/
├── Cargo.toml                      # workspace + dependency catalog
├── rust-toolchain.toml             # pinned to 1.95.0
├── clippy.toml
├── deny.toml                       # cargo-deny config (licenses + advisories)
├── lefthook.yml                    # pre-push hook: fmt + clippy + Conventional Commits
├── crates/
│   ├── kleya-core/                 # domain types, ports, commands. I/O-free.
│   │   ├── src/error.rs            # one Error enum per crate (thiserror)
│   │   ├── src/limits.rs           # named constants — every magic number lives here
│   │   ├── src/model/              # InstanceId, KeyName, TemplateSpec, ...
│   │   ├── src/ports/              # trait CloudCompute, KeyStore, Clock, IdGen
│   │   ├── src/commands/           # one file per subcommand (orchestration only)
│   │   ├── src/bootstrap/          # pure render + encode for user-data
│   │   ├── src/test_support/       # InMemoryCompute, InMemoryKeyStore (feature-gated)
│   │   └── src/util.rs             # wait_or_cancel — shared async helper
│   ├── kleya-aws/                  # CloudCompute impl backed by aws-sdk-ec2
│   │   ├── src/ec2.rs
│   │   ├── src/client.rs
│   │   └── tests/floci/            # Floci-backed integration test
│   ├── kleya-cli/                  # the `kleya` binary
│   │   ├── src/main.rs             # ~30 lines: parse → SIGINT handler → dispatch
│   │   ├── src/clap_args.rs        # clap derive types only
│   │   ├── src/config_loader.rs    # multi-format loader
│   │   ├── src/dispatch.rs         # match Cmd { ... } orchestration
│   │   ├── src/ssh_probe.rs        # probe_ssh_ready + wait_cloud_init
│   │   └── src/key_store_fs.rs     # filesystem KeyStore impl
│   └── kleya-bootstrap-assets/     # embeds setup_devbox.sh.j2 + ghostty.terminfo
├── docs/
│   └── superpowers/
│       ├── specs/                  # design specs (the source of truth)
│       └── plans/                  # implementation plans (one per spec)
├── .github/workflows/
│   ├── ci.yml                      # fmt, clippy, nextest, deny, llvm-cov, floci
│   └── release.yml                 # cargo-dist; tag-triggered
├── ONBOARDING.md                   # agent handoff for the bootstrap plan
├── README.md                       # end-user docs
└── CONTRIBUTING.md                 # this file
```

**Architectural rule:** `kleya-core` is I/O-free and provider-SDK-free. New I/O goes through a port (trait in `kleya-core/src/ports/`) implemented by an adapter crate. This is what lets us swap AWS for another provider in a separate crate without touching `kleya-core`.

## Required tooling

```bash
rustup show active-toolchain   # 1.95.0 (pinned by rust-toolchain.toml)
cargo nextest --version        # cargo install cargo-nextest --locked
cargo llvm-cov --version       # cargo install cargo-llvm-cov --locked
cargo deny --version           # cargo install cargo-deny --locked
dist --version                 # cargo install cargo-dist --locked (only for releases)
jj --version                   # https://github.com/jj-vcs/jj  — preferred VCS
lefthook version               # brew install lefthook   — pre-push hook runner
docker --version               # optional; required only for local Floci tests
gh --version                   # for pushing tags + interacting with releases
```

`jj` is preferred but optional — the repo is a colocated jj-on-git checkout, so plain git works too. Agents should use jj.

## Local setup

```bash
git clone https://github.com/antstanley/kleya.git
cd kleya

# Sanity-check the jj overlay (.jj/ is committed):
jj status                       # if not a jj repo: jj git init --colocate

# Install pre-push hook
lefthook install

# Authenticate gh for pushing
gh auth setup-git
```

## Building + testing

```bash
# Build everything
cargo build --workspace

# Full test suite (what CI runs)
cargo nextest run --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo deny check
cargo llvm-cov --workspace --fail-under-lines 50

# Run a single test
cargo nextest run -p kleya-core --lib error::tests::cancelled_display_contains_instance

# Per-crate test runs need the test-support feature for kleya-core:
cargo nextest run -p kleya-core --features test-support
```

### Floci integration tests (optional)

`crates/kleya-aws/tests/floci/template_lifecycle.rs` is marked `#[ignore]` and exercises the AWS adapter against [Floci](https://floci.io/), a local AWS emulator. Requires Docker:

```bash
docker run -d --rm --name kleya-floci -p 4566:4566 \
  floci/floci@sha256:43f48b8cd04354f356b859cc43a8915a88516df6530d4691159bed39b7e9ea32

KLEYA_TEST_FLOCI=1 KLEYA_TEST_FLOCI_ENDPOINT=http://localhost:4566 \
AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test AWS_REGION=eu-west-1 \
  cargo nextest run -p kleya-aws --run-ignored all
```

> **Known limitation:** Floci doesn't currently implement `CreateLaunchTemplate`. The CI floci job runs with `continue-on-error: true` until upstream support lands or the test is rewritten against the supported subset.

## Coding conventions (Tiger-style)

Strictly enforced — clippy + lefthook will reject violations.

- **70-line function cap** (clippy::too-many-lines = warn → deny via workspace config).
- **100-column line cap.**
- **Two assertions per function minimum** for non-trivial functions (one input precondition, one output postcondition).
- **No `unwrap` / `expect` / `panic!` / `todo!` / `unimplemented!`** in production paths. Workspace clippy config denies them. Use `#[allow(...)]` on test files only.
- **All bounds are named constants** in `crates/kleya-core/src/limits.rs`. Magic numbers anywhere else are a smell.
- **One `Error` enum per crate** via `thiserror`. Adapter errors wrap into `kleya_core::Error::Adapter { provider, source }` at the public boundary. `kleya-core` never sees provider SDK error types.
- **Newtypes at the boundary** (`InstanceId`, `KeyName`, `Region`, etc.) validate at construction. Domain code receives the validated newtype, not `String`.
- **Idempotent `ensure_default_*` methods** on `CloudCompute`. Adapters treat provider-side "already exists" as success after a follow-up `Describe*` confirms the resource matches.

If you need to break a rule (e.g. interior `expect` on a `Lazy<Regex>` that can't fail), gate it with a narrow `#[allow(...)]` and a comment explaining why.

## Testing conventions

- **Unit tests** live in `#[cfg(test)] mod tests` blocks alongside the code they test.
- **Integration tests** (in `crates/*/tests/`) exercise public APIs and orchestration with `InMemoryCompute` + `InMemoryKeyStore` fakes from `kleya_core::test_support` (feature-gated).
- **Boundary tests** — at any named limit, test `value - 1`, `value`, `value + 1`.
- **Negative space** — every limit, every regex, every parser gets at least one positive and one negative test.
- **No mocking the database, no mocking IPC.** Use the in-memory fakes that implement the port traits.
- **Snapshot tests** (via `insta`) for the rendered user-data script. Update with `cargo insta review` only after a deliberate change to the template or its variables.

## VCS workflow (jj-first; git equivalents in parens)

`main` is the integration bookmark. Push directly to `main` — no PR-required workflow, but PRs work fine if you want one.

```bash
# Make changes…
# (jj auto-snapshots the working copy on every command)

# Describe the current revision
jj describe -m "feat(scope): subject"           # git: git commit -m "..."

# Start a new revision on top
jj new                                           # git: (n/a — jj is changeset-oriented)

# Advance the integration bookmark
jj bookmark move main --to @-                    # git: (already at HEAD)

# Push when you're ready
jj git push --bookmark main                      # git: git push origin main
```

### Conventional Commits

Enforced by lefthook's commit-msg hook. Subject prefixes:

- `feat:` / `feat(scope):` — new capability
- `fix:` / `fix(scope):` — bug fix
- `refactor:` — change shape, not behavior
- `test:` — tests only
- `docs:` — docs only
- `chore:` — manifest, tooling, deps
- `ci:` — workflow changes
- `release:` — version bumps / release prep

Body should explain the "why" — the "what" is in the diff.

## CI pipeline

`.github/workflows/ci.yml` runs on every push to `main` and on PRs:

| Step | Blocks merge? |
|---|---|
| `cargo fmt --check` | Yes |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Yes |
| `cargo nextest run --workspace` | Yes |
| `cargo deny check` | Yes |
| `cargo llvm-cov --workspace --fail-under-lines 50` | Yes |
| Floci integration test (`floci` job) | **No** (continue-on-error; see above) |

All third-party actions are pinned to commit SHAs with a human-readable version comment. When you update an action, look up the new SHA via `gh api repos/<owner>/<repo>/git/refs/tags/<tag> -q '.object.sha'` (deref the tag object for annotated tags).

## Release process (cargo-dist)

Releases are driven by tag pushes. The workflow is in `.github/workflows/release.yml`, generated by `dist generate --mode=ci`.

### Cutting a release

```bash
# 1. Bump versions across the workspace
sed -i 's/version = "0.1.0-rc.1"/version = "0.1.0"/' crates/*/Cargo.toml
cargo update --workspace            # refresh Cargo.lock

# 2. Verify dist is happy
dist plan                           # prints the artifact matrix

# 3. Commit + push the bump
jj describe -m "release: v0.1.0"
jj bookmark move main --to @-
jj git push --bookmark main         # CI runs

# 4. After CI is green, tag and push (use git for annotated tags)
git tag -a v0.1.0 main -m "v0.1.0"
git push origin v0.1.0              # triggers release.yml

# 5. Wait for the release workflow (~30 min — macOS LTO dominates).
#    It produces:
#      - kleya-cli-{x86_64,aarch64}-{unknown-linux-gnu,apple-darwin}.tar.xz
#      - kleya-cli-installer.sh
#      - source.tar.gz
#      - sha256.sum
#    Auto-publishes a GitHub Release at /releases/tag/v0.1.0
#    (prerelease flag is auto-set if the version has a -rc/-alpha/-beta suffix).
```

### After `dist generate`

If you ever re-run `dist generate --mode=ci` (e.g. when bumping `cargo-dist-version`), the regenerated `release.yml` uses unpinned action refs (`actions/checkout@v6` etc.). **Re-apply the SHA pins** so the release workflow stays consistent with `ci.yml`'s supply-chain policy. The `allow-dirty = ["ci"]` setting in `[workspace.metadata.dist]` lets dist tolerate this; without it, `dist host` would abort on the tag push.

## Things in flux

- **Floci `CreateLaunchTemplate`.** The `floci` CI job runs in advisory mode (`continue-on-error: true`). Re-gate (drop the flag) once Floci ships the op or the test is rewritten.
- **`rustls-webpki` advisories** (RUSTSEC-2026-0098 / 0099 / 0104). Currently ignored in `deny.toml` because upstream hasn't shipped patched 0.101.x / 0.103.x. Remove the ignores when `cargo update` resolves to fixed versions.

## Pointers

- **Design spec:** [`docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md`](docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md) — the source of truth for what's intended and why.
- **Implementation plan:** [`docs/superpowers/plans/2026-05-16-kleya-bootstrap.md`](docs/superpowers/plans/2026-05-16-kleya-bootstrap.md) — task-by-task build order.
- **Review-fix design + plan:** [`docs/superpowers/specs/2026-05-18-review-fixes-design.md`](docs/superpowers/specs/2026-05-18-review-fixes-design.md), [`docs/superpowers/plans/2026-05-18-review-fixes.md`](docs/superpowers/plans/2026-05-18-review-fixes.md) — the most recent batch of corrections.
- **Agent handoff:** [`ONBOARDING.md`](ONBOARDING.md) — for fresh agent sessions picking up the bootstrap plan.

If you're writing a new spec, follow the format of the existing one (numbered sections, named limits, lifecycle tables for state machines). If you're writing a plan, follow `docs/superpowers/plans/2026-05-18-review-fixes.md` — bite-sized steps, explicit verification, conventional commit per task.
