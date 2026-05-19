# `kleya` — SSO Login Design

**Status:** Draft · **Date:** 2026-05-19 · **Owner:** Ant Stanley · **Scope:** Repo-wide

A draft design for enabling AWS IAM Identity Center (SSO) interactive login from inside `kleya`, by opting into the `credentials-login` Cargo feature on `aws-config`. This document describes the proposed surface; the canonical specs at [../../specs/](../../specs/) will be updated when the work lands. Until then, the on-disk behaviour is: SSO **token consumption** works today (via the default-enabled `sso` feature on `aws-config`), but kleya cannot drive the **interactive login** itself — operators have to shell out to `aws sso login` before running `kleya`.

---

## 1. Purpose

Today, an operator who authenticates with AWS IAM Identity Center has to bounce between two CLIs:

```
aws sso login --profile devbox-sso     # one shell
kleya launch --profile devbox-sso      # another (or the same after the first call)
```

The first call writes a token under `~/.aws/sso/cache/`; kleya's `aws-config` instance picks it up transparently through the credentials chain. This works, but:

- The operator must install the AWS CLI v2 (a Python-bundled binary, ~80 MB) for the *only* purpose of running `sso login`.
- The first-time UX is poor — kleya errors with a generic `Adapter` failure (exit 70) when the cache is missing or expired; the actual remediation is documented nowhere on the kleya surface.
- The token refresh cycle (default ~8 hours) drops back to the same opaque failure mode.

`aws-config 1.8.x` exposes an opt-in `credentials-login` feature that bundles the `aws-sdk-signin` crate plus the crypto deps (`p256`, `sha2`, `base64-simd`, `uuid`, `rand`, `zeroize`) needed to drive the device-authorization OIDC flow directly from Rust. Enabling it lets kleya own the whole loop: detect a missing/expired token, prompt the operator with the verification URL, poll for the issued token, and write it to the same cache directory `aws-sdk-sso` reads from. The AWS CLI v2 dependency goes away.

---

## 2. Changes

### 2.1 Cargo feature wiring

[`crates/kleya-aws/Cargo.toml`](../../../crates/kleya-aws/Cargo.toml) and [`crates/kleya-cli/Cargo.toml`](../../../crates/kleya-cli/Cargo.toml):

```toml
aws-config = { version = "1", features = ["credentials-login"] }
```

`aws-config`'s defaults (`default-https-client`, `rt-tokio`, `credentials-process`, `sso`) remain enabled implicitly — `credentials-login` is additive, not a replacement. The new transitive crates compiled in are listed at [docs.rs/aws-config](https://docs.rs/aws-config) and are all maintained by the AWS SDK team or the `RustCrypto` org; `cargo deny check` must continue to pass after the bump.

**Binary-size note.** The opt-in feature adds roughly 1.2 MB of crypto + signin code to the release binary on `aarch64-unknown-linux-gnu` (measured against `cargo-bloat` at the time of writing). Within the existing ~14 MB release artefact this is acceptable; the design does not pursue conditional compilation behind a kleya-level Cargo feature.

### 2.2 New `kleya sso` subcommand

[`crates/kleya-cli/src/clap_args.rs`](../../../crates/kleya-cli/src/clap_args.rs):

```
kleya sso login    [--profile <p>] [--start-url <u>] [--region <r>] [--no-browser]
kleya sso logout   [--profile <p>] [--all]
kleya sso status   [--profile <p>] [--json]
```

| Command | Effect |
|---|---|
| `sso login` | Run the OIDC device-authorization flow. Prints the verification URL + user code on stderr, optionally opens the browser via `open` / `xdg-open` (suppressed by `--no-browser`), polls the token endpoint, writes the access + refresh token to `~/.aws/sso/cache/<hash>.json`. Idempotent: a still-valid cached token short-circuits with `tracing::info!("sso cache hit", profile, expires_in)`. |
| `sso logout` | Remove the cached token for `--profile` (or every cached profile when `--all`). Calls the SSO `Logout` API server-side before deleting the local file. |
| `sso status` | Print the cached token's profile, account id, role name, start url, expiry. `--json` switches to a single-object payload using the same field naming as the file format. |

**Profile resolution.** `--profile` follows the same precedence as the rest of the CLI (CLI flag → `KLEYA_PROFILE` env → `AWS_PROFILE` env → `default_profile` in `Config`, see [03-configuration.md](../../specs/03-configuration.md#precedence)). The profile must be declared in `~/.aws/config` with the standard SSO fields (`sso_session` or `sso_start_url` + `sso_region` + `sso_account_id` + `sso_role_name`). When `--start-url` / `--region` are passed explicitly, they override the profile values for the call but are not written back to `~/.aws/config`.

**Browser launch.** `open` on macOS and `xdg-open` on Linux. Failure to spawn is non-fatal — the verification URL is always printed on stderr regardless. `--no-browser` skips the launch attempt entirely (useful in headless CI / SSH sessions).

### 2.3 Configuration

No new `[sso]` section in `Config`. Kleya leans on the profile-based config that `aws-config` already reads from `~/.aws/config`. Two reasons:

1. Duplicating `sso_start_url`, `sso_region`, `sso_account_id`, `sso_role_name` in `~/.config/kleya/config.toml` would drift over time and break the principle that **kleya configures kleya; AWS configures AWS**.
2. Operators who use SSO with multiple AWS tools (cdk, sam, terraform, ec2-instance-connect) already have the profile set up. Kleya should reuse it.

A new optional field `default_sso_profile: Option<String>` on the top-level `Config` is **not** added. If the operator's chosen `default_profile` happens to be an SSO profile, `kleya sso login` uses it; if it's a static-credentials profile, `kleya sso login` returns `Error::ConfigInvalid { reason: "profile <p> has no sso_start_url" }`.

### 2.4 Credentials chain

No change to the existing client builders ([`build_ec2_client`](../../../crates/kleya-aws/src/client.rs)). `aws-config`'s default chain already prefers SSO when the profile has it. The wiring change is purely:

- The new `kleya sso login` subcommand calls `aws_config::sso::login::Builder::new()`-style API (or whatever final shape the `credentials-login` feature exposes) directly. The output is the same token cache file `aws-sdk-sso` reads from inside the default chain on the next `RunInstances` call.

This means:

- Pre-existing kleya commands (`launch`, `connect`, `terminate`, `list`, `template *`) gain SSO support transparently — once `kleya sso login` has run, the cached token flows through to every subsequent command via the unchanged default chain.
- No `kleya-core` change. SSO is an authentication concern; the `CloudCompute` trait operates one layer below it.

### 2.5 Module layout

New file: `crates/kleya-cli/src/sso.rs` (does not exist yet).

```
kleya-cli/src/
  sso.rs              # subcommand entry points: login(), logout(), status()
  sso_token_cache.rs  # path resolution + atomic write under ~/.aws/sso/cache/
```

Two functions are unit-testable without a real SSO endpoint:

- `cache_path(start_url: &str) -> PathBuf` — SHA-1 of the start url, hex, plus `.json` extension. Asserted against a fixture (`https://my-sso.awsapps.com/start/` → `c0...{38 hex chars}.json`). Matches the AWS CLI v2 layout so the two tools share a cache.
- `verify_url_message(url: &str, code: &str) -> String` — pure formatter for the stderr prompt. Snapshot-tested.

The interactive polling loop (which depends on `aws-sdk-signin`) is integration-tested only.

### 2.6 Errors and exit codes

New `Error::SsoLoginFailed { reason: String }` variant in [`crates/kleya-core/src/error.rs`](../../../crates/kleya-core/src/error.rs). Exit code **8** — the next free integer in the existing 0–7 + 70 + 74 + 130 sequence (see [02-cli-surface.md](../../specs/02-cli-surface.md#exit-codes)).

`reason` is one of:

- `"profile <p> has no sso_start_url"` (config gap).
- `"device authorization timed out after <s>s"` (operator never visited the URL).
- `"token cache write failed: <io error>"` (filesystem issue).
- `"sso endpoint returned: <code> <message>"` (API surface — passed through unmodified).

`Error::Cancelled` (Ctrl-C during the polling loop) is the existing variant, exit 130.

### 2.7 Testing

| Tier | Scope | Location | Gating |
|---|---|---|---|
| Unit | `cache_path` SHA-1 compatibility against the AWS-CLI-v2 cache file naming; `verify_url_message` formatting | `crates/kleya-cli/src/sso_token_cache.rs::tests`, `crates/kleya-cli/src/sso.rs::tests` | always-on |
| Integration (fakes) | `Error::SsoLoginFailed` rendering + exit-code mapping (`code_for(&e) == 8`) | `crates/kleya-cli/src/exit_code.rs::tests` | always-on |
| e2e (manual) | Drive `kleya sso login` against a real IAM Identity Center instance | not committed; documented in CONTRIBUTING.md | manual |

Floci has no SSO endpoint emulation, so there is no Floci tier for this work.

---

## 3. Out of scope

- **Storing AWS profile config (`~/.aws/config`) on kleya's behalf.** Operators continue to manage profiles via `aws configure sso` or hand-editing.
- **Static-credentials login (`aws configure`).** Static credentials remain supported via the SDK chain; no kleya CLI surface for them.
- **Programmatic role assumption / `sts assume-role`.** A separate concern; the SDK chain already handles the `source_profile = …` / `role_arn = …` pattern.
- **A kleya-level Cargo feature to disable SSO.** The feature is always-on in the released binary. Embedded / size-sensitive forks can vendor `aws-config` with their own feature set.
- **Updating `~/.aws/config` from kleya** (e.g., a `kleya sso configure` wizard). Could be added later but is not required for `credentials-login` to function.

---

## 4. Acceptance

- `cargo build --workspace --release` succeeds with the new feature enabled.
- `cargo deny check` passes against the expanded dependency tree.
- `cargo test --workspace` passes, including the new unit tests.
- Manual: `kleya sso login --profile <real-sso-profile>` prints a verification URL, accepts the code in the browser, writes a file under `~/.aws/sso/cache/`, and exits 0. A subsequent `kleya list` succeeds without `aws sso login` being involved.
- Manual: `aws sso login --profile <p>` followed by `kleya sso status --profile <p>` shows the cached token, confirming kleya and the AWS CLI v2 share the cache.
- Failure of any individual `kleya sso *` command surfaces as exit code 8 (`SsoLoginFailed`) or 130 (`Cancelled`); no `Adapter` (70) fall-through.

---

## 5. Open questions

- *Cache-file lock contention.* When the operator runs `kleya sso login` concurrently with the AWS CLI v2 against the same profile, both tools may try to write the same cache file. The CLI v2 takes an `flock`-style advisory lock; kleya should match. Decide whether to lift the implementation from `aws-sdk-signin`'s internal helper or re-implement against `fs2`.
- *Refresh on demand inside other subcommands.* Today, an expired token surfaces as an `Adapter` error mid-`launch`. Should `dispatch::run` detect this and prompt the operator to re-login inline, or keep the surfaces strictly separated (`kleya sso login` is the only place the device-authorization flow runs)? The strict separation is simpler and avoids interleaving a TTY prompt into a long-running command; revisit if operator feedback flags it.
- *Multi-account browsing.* IAM Identity Center supports listing accounts/roles via `aws sso list-accounts` after auth. A `kleya sso accounts` / `kleya sso roles` browser would be a quality-of-life addition; deferred until first concrete ask.
- *Headless / CI use.* CI systems typically use static OIDC role assumption via `assume-role-with-web-identity`, not interactive SSO. Confirm the credentials chain works end-to-end with a GitHub Actions OIDC token *and* a kleya release binary that has `credentials-login` enabled — the additive features should not interfere with non-SSO paths, but verify before promotion to `Implemented`.
