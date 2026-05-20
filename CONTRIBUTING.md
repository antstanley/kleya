# Contributing to kleya

This guide is written for both humans and coding agents. It covers the repo layout, required tooling, coding conventions, the test/CI gate, and the release process. The canonical design specification lives in [`docs/specs/`](docs/specs/) — eleven numbered markdown pages plus a JSON Schema sidecar, indexed by [`docs/README.md`](docs/README.md). Read [`docs/specs/00-overview.md`](docs/specs/00-overview.md) before making non-trivial changes; the historical drafts in [`docs/superpowers/specs/`](docs/superpowers/specs/) are kept for context only.

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
│   ├── README.md                   # index of every spec page + historical drafts
│   ├── specs/                      # CANONICAL design spec (00-overview → 11-credentials-and-sso)
│   └── superpowers/
│       ├── specs/                  # historical drafts; canonical set supersedes them
│       └── plans/                  # implementation plans (one per change)
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

`crates/kleya-aws/tests/floci/template_lifecycle.rs` and `instance_lifecycle.rs` are marked `#[ignore]` and exercise the AWS adapter against [Floci](https://floci.io/), a local AWS emulator. Requires Docker:

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

## Mutation testing

Validator regressions are easy to miss with normal tests — a constructor that silently accepts invalid input still passes the happy-path test. We use [cargo-mutants](https://mutants.rs/) to catch this.

```bash
cargo install cargo-mutants
cargo mutants
```

Scope and exclusions live in `.mutants.toml`. The aim is **no surviving mutants** on the listed files; if a change adds a new domain-type constructor or parser, extend `examine_globs` and add tests that kill any new mutants. Not yet wired into CI.

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

A version bump touches six files: the four `crates/*/Cargo.toml`, `Cargo.lock` (the four workspace-member entries), and `README.md` (the status line, the two installer URLs, and the `--version` pin). Keep them in lockstep.

```bash
# 1. Bump versions across the workspace (OLD -> NEW, e.g. 0.1.0-rc.2 -> 0.1.0-rc.3)
OLD=0.1.0-rc.2; NEW=0.1.0-rc.3
sed -i "s/$OLD/$NEW/g" crates/*/Cargo.toml README.md   # macOS: sed -i ''
cargo update --workspace            # refresh Cargo.lock's workspace entries

# 2. Verify dist is happy
dist plan                           # prints the artifact matrix

# 3. Commit + push the bump
jj describe -m "release: v$NEW"     # @ holds the bump
jj new                              # new empty working copy on top
jj bookmark set main -r @-          # point main at the release commit
jj git push --bookmark main         # CI runs

# 4. After CI is green, create + push the tag.
#    jj creates the tag (colocated repo exports it to refs/tags/), but
#    `jj git push` can't push tags (jj 0.41) — so push it with git.
jj tag set v$NEW -r main            # or: git tag -a v$NEW main -m "v$NEW"
git push origin v$NEW               # triggers release.yml

# 5. Wait for the release workflow (~30 min — macOS LTO dominates).
#    It produces:
#      - kleya-cli-{x86_64,aarch64}-{unknown-linux-gnu,apple-darwin}.tar.xz
#      - kleya-cli-installer.sh
#      - source.tar.gz
#      - sha256.sum
#    Auto-publishes a GitHub Release at /releases/tag/v$NEW
#    (prerelease flag is auto-set if the version has a -rc/-alpha/-beta suffix).
```

### After `dist generate`

If you ever re-run `dist generate --mode=ci` (e.g. when bumping `cargo-dist-version`), the regenerated `release.yml` uses unpinned action refs (`actions/checkout@v6` etc.). **Re-apply the SHA pins** so the release workflow stays consistent with `ci.yml`'s supply-chain policy. The `allow-dirty = ["ci"]` setting in `[workspace.metadata.dist]` lets dist tolerate this; without it, `dist host` would abort on the tag push.

## Things in flux

- **Floci `CreateLaunchTemplate`.** The `floci` CI job runs in advisory mode (`continue-on-error: true`). Re-gate (drop the flag) once Floci ships the op or the test is rewritten.
- **`rustls-webpki` advisories** (RUSTSEC-2026-0098 / 0099 / 0104). Currently ignored in `deny.toml` because upstream hasn't shipped patched 0.101.x / 0.103.x. Remove the ignores when `cargo update` resolves to fixed versions.

## Pointers

- **Canonical design spec:** [`docs/specs/`](docs/specs/) (indexed by [`docs/README.md`](docs/README.md)) — 11 numbered pages covering overview, domain model, CLI surface, configuration, provider port, bootstrap, launch+connect, error model, testing, architecture principles, development guidelines, and credentials+SSO; plus `canonical-types.schema.json`. Every page closes with `Assumptions / Decisions / Open questions`. This is the source of truth for what is checked into `main`.
- **In-flight drafts and implementation plans:** [`docs/superpowers/specs/`](docs/superpowers/specs/) and [`docs/superpowers/plans/`](docs/superpowers/plans/). Drafts propose changes; plans drive their implementation. When a draft lands, it's promoted into `docs/specs/` (as a new page or as edits to an existing one) and the draft is either deleted or marked `Status: Withdrawn`. See [`docs/README.md`](docs/README.md#historical-drafts) for the current draft inventory.
- **Agent handoff:** [`ONBOARDING.md`](ONBOARDING.md) — for fresh agent sessions picking up an in-progress plan.

If you're writing a new canonical spec page, follow the format of the existing numbered files in [`docs/specs/`](docs/specs/): a header (`**Status:** Implemented · **Date:** YYYY-MM-DD · **Owner:** <name>`), one Responsibilities section, body sections that describe what the code does (not what it might do), and the mandatory closing `## Assumptions and open questions` block split into Assumptions / Decisions / Open questions. Status is `Implemented` (the default for the canonical set); use `Draft` only for a clearly-marked design-stage page that will be promoted to `Implemented` when the work lands.

If you're writing an implementation plan, follow [`docs/superpowers/plans/2026-05-19-credentials-login.md`](docs/superpowers/plans/2026-05-19-credentials-login.md) or [`docs/superpowers/plans/2026-05-19-spec-gaps.md`](docs/superpowers/plans/2026-05-19-spec-gaps.md) — bite-sized phases, explicit verification commands, conventional-commit subject per phase, and a documented `jj describe / jj bookmark move main / jj new / jj git push -b main` sequence at the end of each phase.
