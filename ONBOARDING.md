# Onboarding — `kleya`

You are an agent picking up an implementation. Read this top-to-bottom before touching any code.

## What this repo is

A Rust CLI (`kleya`) that bootstraps AWS spot instances for agentic coding environments. Workspace of four crates with a `CloudCompute` port so non-AWS providers can be added later. See the spec for the full design.

- **Spec (read first):** [`docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md`](docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md)
- **Plan (read second, then execute):** [`docs/superpowers/plans/2026-05-16-kleya-bootstrap.md`](docs/superpowers/plans/2026-05-16-kleya-bootstrap.md)

## Execution flow

1. Invoke `superpowers:using-superpowers` to establish the skill flow (the system reminder will surface available skills).
2. Then invoke **either**:
   - `superpowers:subagent-driven-development` — recommended; dispatches a fresh subagent per task with review checkpoints.
   - `superpowers:executing-plans` — inline execution in one session.
3. Work the plan task-by-task. Each task is TDD-shaped (red → green → verify → commit). Do not skip ahead; later tasks assume earlier types and signatures.
4. After every task's verify passes, commit per the **Per-task commit convention** at the top of the plan, then `jj new` to start the next.

## Required tooling (verify before Task 1)

```bash
rustup show active-toolchain   # 1.95.0 expected — rust-toolchain.toml pins it
cargo nextest --version        # cargo install cargo-nextest --locked
cargo llvm-cov --version       # cargo install cargo-llvm-cov --locked
jj --version                   # brew install jj
lefthook version               # brew install lefthook
docker --version               # required for Floci AWS-shaped tests (Task 17, optional locally)
gh auth status                 # for pushing to the remote
```

## Repository conventions (non-negotiable — see [`docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md`] §10–§15)

- **Tiger-style Rust:** 70-line function cap, 100-column line cap, two assertions per function minimum, no `unwrap` / `expect` / `panic!` in production paths. All bounds are named consts in `kleya-core/src/limits.rs` — no magic numbers anywhere else.
- **Errors:** one `Error` enum per crate via `thiserror`. Adapter errors wrap into `kleya_core::Error::Adapter { provider, source }` at the public boundary. Core never sees provider SDK error types.
- **Testing:** `cargo nextest run` is the only sanctioned runner. Test positive AND negative space — at any limit, test `value-1`, `value`, `value+1`. 50% line coverage floor (`cargo llvm-cov`).
- **VCS:** jj (jujutsu) on git backend. `main` is the integration bookmark. Conventional Commits enforced. Never `jj abandon` / `jj op restore` / `jj git fetch --force` without explicit user confirmation. The repo is colocated (`.jj` + `.git`) so git tooling works.
- **Architectural rule:** `kleya-core` is I/O-free and provider-SDK-free. New I/O goes through a port (trait in `kleya-core::ports`) implemented by an adapter crate.
- **No README files / extra docs.** This `ONBOARDING.md` is an agent handoff, not user docs — leave it as-is.

## Local-environment one-time setup

```bash
git clone https://github.com/antstanley/kleya.git
cd kleya
# colocated jj is already set up via .jj/; sanity-check:
jj status
# install pre-push hook
lefthook install
# auth for pushes (skip if already configured)
gh auth setup-git
```

If `jj status` reports it's not a jj repo (e.g., you cloned with git only and `.jj` is in `.gitignore`), run `jj git init --colocate` to add the jj overlay without disturbing git state.

## Where to find external context

- Bootstrap shell script (origin material for the embedded user-data template): https://gist.github.com/antstanley/2fb19f038ca984093954c7e3f0d508ae
- Development guidelines (the full Tiger-style rule set this plan was written against): https://gist.github.com/antstanley/5bdaa85e63427fadae1c58ae6db77c27
- Ghostty terminfo source (pin by commit SHA in the asset header): https://raw.githubusercontent.com/ghostty-org/ghostty/main/include/ghostty.terminfo
- Floci (Docker AWS emulator used for the integration test tier; replaces LocalStack): https://floci.io/

## If something in the plan doesn't compile

The `aws-sdk-ec2` crate is on a fast release cadence; method names like `tags()` / `instance_id()` / `InstanceType::from(...)` may have shifted by the time you run `cargo build`. If a call signature differs:

1. **Do not downgrade the SDK** — keep `aws-sdk-ec2 = "1"`.
2. Consult the published docs for the current 1.x release and adapt the call. The structure of the adapter (the trait impl boundaries) stays the same.
3. If the conceptual operation has been removed (very unlikely on a stable 1.x), surface it — don't silently no-op.

## Hand-off back to the user

Each task is independently reviewable. After completing a task you trust:

```bash
jj describe -m "<conventional commit subject>"
jj new                          # start the next task in a fresh working revision
jj bookmark move main --to @-   # advance the integration bookmark
jj git push --bookmark main     # push when the user asks (do not push unprompted)
```

Do not push to `origin` without explicit user instruction.
