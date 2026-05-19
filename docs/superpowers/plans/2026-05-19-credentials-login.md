# Enable `credentials-login` on `aws-config` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:executing-plans`. Steps use checkbox (`- [ ]`) syntax. Repo is jj-managed; per-phase VCS sequence is documented below.

**Goal:** Close the Open Question on `docs/specs/11-credentials-and-sso.md` by enabling the `credentials-login` Cargo feature on the workspace `aws-config` dependency. After this change, profiles written by `aws login` (AWS CLI v2 ≥ 2.32.0) — i.e., profiles carrying `login_session = arn:aws:iam::…:user/...` — are consumed natively through the default credentials chain via `aws_config::login::LoginCredentialsProvider`. No CLI surface changes; no code changes beyond the two `Cargo.toml` lines.

**Architecture:** Two-line additive feature flip. The `LoginCredentialsProvider` and the `BaseProvider::LoginSession` dispatcher arm in `aws-config` 1.8.x already exist and are gated `#[cfg(feature = "credentials-login")]` (verified against smithy-rs `adc0a46`). Adding the feature to `kleya-aws/Cargo.toml` and `kleya-cli/Cargo.toml` makes both crates' `aws-config` instances eligible; cargo unifies the feature set across the workspace, so a single enable site is technically sufficient, but explicit per-crate features make the dependency self-describing. After the flip, the binary gains `aws-sdk-signin` + RustCrypto crates (`p256`, `sha2`, `base64-simd`, `uuid`, `rand`, `zeroize`).

**Tech stack:** Rust 1.95.0, Cargo, `cargo-deny`. VCS: jj on git backend.

---

## Per-phase VCS sequence (jj-only)

After every phase's verification step passes:

```bash
jj describe -m "<conventional-commit subject>"
jj bookmark move main --to @
jj new
jj git push -b main
```

| Phase | Subject |
|---|---|
| 0 | `docs(plan): add 2026-05-19 credentials-login implementation plan` |
| 1 | `feat(aws): enable credentials-login feature on aws-config` |
| 2 | `docs(spec): close credentials-login open question; mark consumer enabled` |

---

## Phase 0 — Commit the plan

- [ ] **Step 1:** Confirm this file exists at `docs/superpowers/plans/2026-05-19-credentials-login.md`.
- [ ] **Step 2:** `jj status` — only this file should be modified in `@`.
- [ ] **Step 3:** Commit + push.

---

## Phase 1 — Enable the Cargo feature

### Files

- Modify: `crates/kleya-aws/Cargo.toml`
- Modify: `crates/kleya-cli/Cargo.toml`

### Step 1: Flip the feature

Both crates currently declare `aws-config = "1"`. Replace with:

```toml
aws-config = { version = "1", features = ["credentials-login"] }
```

No other line changes; this is purely additive.

### Step 2: Build the workspace

```
cargo build --workspace --all-targets
```

Expected: success. New transitive crates compile (`aws-sdk-signin`, `p256`, `sha2`, `base64-simd`, `uuid`, `rand`, `zeroize` and their own transitives). Roughly +1.2 MB of code paths into the release build.

### Step 3: Run the test suite

```
cargo test --workspace
```

Expected: all tests pass — no test exercises the credentials path directly, so the feature flip is transparent.

### Step 4: Clippy

```
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: clean. The new code is in `aws-config`, not in kleya; lints apply to our code only.

### Step 5: `cargo deny check`

```
cargo deny check
```

Expected: clean. The new transitive crates are MIT / Apache-2.0 / BSD-3-Clause / ISC / RustCrypto-typical licences, all covered by the `[licenses] allow` list in `deny.toml`. If a new license shows up, add it to `deny.toml`'s allow list with a one-line justification.

If `cargo deny check` surfaces a new advisory or yanked-version warning specific to the new crates, follow the existing pattern in `deny.toml` (the `[advisories] ignore` list already documents a few rustls-webpki advisories with links and remove-when conditions). Add an ignore only after a quick look at the advisory text; do not blanket-suppress.

### Step 6: Format check

```
cargo fmt --all -- --check
```

Expected: no diff. Cargo.toml is not formatted by `rustfmt`; this step verifies no other files moved.

### Step 7: Commit + push

```
jj describe -m "feat(aws): enable credentials-login feature on aws-config"
jj bookmark move main --to @
jj new
jj git push -b main
```

---

## Phase 2 — Update the spec docs

### Files

- Modify: `docs/specs/11-credentials-and-sso.md`
- Modify: `docs/superpowers/specs/2026-05-19-sso-login-design.md`

### Step 1: `docs/specs/11-credentials-and-sso.md`

- Intro paragraph — drop the "via the `credential_process` shim otherwise" parenthetical; `aws login` is now natively supported.
- Credentials-sources table row for `aws login` — drop the "**when the `credentials-login` Cargo feature is enabled**" caveat; the feature is now enabled.
- "Browser-based login → `aws login`" section — rewrite the "Current state in kleya" subsection to say the feature is enabled; remove the `credential_process` shim block (or keep as historical "previously required a shim" footnote — operator-friendly). Source-reference links to smithy-rs stay.
- "What kleya does and does not touch" — `~/.aws/login/cache/<sha256>` is now read **directly** by `aws-config`, not indirectly via the shim.
- Decisions — promote the "enable credentials-login" item from Open Questions into a Decision. New Decision: *Enable `credentials-login` on `aws-config`.* **Done.** The feature gates both `LoginCredentialsProvider` (the consumer kleya needs) and `aws-sdk-signin` (the producer kleya does not wire into any CLI surface). Rationale: native `login_session` profile consumption with no CLI surface change; binary size hit of ~1 MB is acceptable.
- Open Questions — remove the "Enable the `credentials-login` feature" item.

### Step 2: `docs/superpowers/specs/2026-05-19-sso-login-design.md`

- Update the "What this withdrawal does not decide" section: the credentials-login feature is now enabled. Cite this plan and commit.

### Step 3: Cross-link sweep

```
for f in docs/specs/11-credentials-and-sso.md docs/specs/00-overview.md docs/specs/03-configuration.md docs/README.md docs/superpowers/specs/2026-05-19-sso-login-design.md; do
  grep -oE '\]\(([^)]+)\)' "$f" | sed -E 's/^\]\((.*)\)$/\1/' | while read link; do
    case "$link" in
      http*) ;; \#*) ;;
      *) base=$(dirname "$f"); target="$base/$link"; if [ ! -e "${target%%#*}" ]; then echo "MISSING in $f: $link"; fi;;
    esac
  done
done
```

Expected: no missing links.

### Step 4: Commit + push

```
jj describe -m "docs(spec): close credentials-login open question; mark consumer enabled"
jj bookmark move main --to @
jj new
jj git push -b main
```

---

## Done criteria

- [ ] `cargo build --workspace --all-targets` succeeds with the new feature.
- [ ] `cargo test --workspace` succeeds (53 / 53 tests passing — the existing count after Phase 2 of the spec-gaps plan).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo deny check` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] `jj log -r 'main..@-' --no-graph` empty (every phase advanced through main).
- [ ] `jj git push -b main` reflects the three phases on `origin/main`.
- [ ] `docs/specs/11-credentials-and-sso.md` no longer carries the "Enable the `credentials-login` feature" Open Question.
- [ ] Manual: `cargo run -p kleya-cli -- --help` lists no `kleya login` / `kleya sso login` subcommand (the principle from the withdrawn-draft Decision holds).
