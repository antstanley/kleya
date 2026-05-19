# 11 — Credentials and SSO

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya` does not manage AWS credentials itself. Every adapter call delegates to `aws-config`'s default credentials chain, which resolves to one of the supported sources at runtime: environment variables, an `~/.aws/config` profile, an IAM Identity Center (SSO) cached token, an `aws login` console-credentials session (natively when the `credentials-login` Cargo feature is enabled on `aws-config`, via a `credential_process` shim otherwise), a generic `credential_process` helper, instance-profile metadata (IMDS), or an OIDC web-identity token. This page documents which sources are reachable in the current code, where each one is read from, how kleya's profile / region overrides interact with the SDK's own resolution, and how a credential failure surfaces.

---

## Responsibilities

1. Resolve a single `region` and `profile` per invocation from CLI / env / config (in that precedence order — see [03-configuration.md](03-configuration.md#precedence)).
2. Pass exactly those two strings into `aws-config` via `aws_config::defaults(BehaviorVersion::latest()).region(...)`, optionally `.endpoint_url(...)` for the Floci test path.
3. Let `aws-config` resolve credentials from its default chain; kleya itself reads no `AWS_*` variable, no `~/.aws/credentials` file, and no SSO token cache.
4. Surface every credential-acquisition failure as `Error::Adapter { provider: "aws-ec2", source }` at the public boundary (exit code 70).

The wiring lives in [`crates/kleya-aws/src/client.rs`](../../crates/kleya-aws/src/client.rs) (one function, ~10 lines). The `dispatch::run` entry point in [`crates/kleya-cli/src/dispatch.rs`](../../crates/kleya-cli/src/dispatch.rs) also builds an `SsmClient` the same way for AMI alias resolution.

---

## Supported credential sources

`aws-config 1.8.x` is taken with default features (`default-https-client`, `rt-tokio`, `credentials-process`, `sso`), plus no explicit overrides on the workspace dependency. The default chain in priority order:

| Source | Where it reads from | How an operator opts in |
|---|---|---|
| **Environment variables** | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` | Export the three vars in the shell that runs `kleya` |
| **Profile (static)** | `~/.aws/credentials` `[<profile>]` block | `aws configure` against the chosen profile |
| **SSO (cached token)** | `~/.aws/sso/cache/<sha1>.json` written by `aws sso login` | Run `aws sso login --profile <p>` once per ~8 h. For IAM Identity Center users. |
| **Console credentials (`aws login`)** | `~/.aws/login/cache/<sha256>` written by `aws login`; profile block in `~/.aws/config` has `login_session = arn:aws:iam::…:user/...` | Run `aws login --profile <p>` against an IAM user / root / federated identity (AWS CLI v2 ≥ 2.32.0). Natively supported by `aws-config`'s `LoginCredentialsProvider` **when the `credentials-login` Cargo feature is enabled** on the workspace `aws-config` dependency — see "Browser-based login" below. |
| **Profile credential_process** | `~/.aws/config` `[profile <p>]` with `credential_process = <cmd>` | Configure a helper (`aws-vault exec`, `granted assume`, the `aws login` shim, …) that prints JSON on stdout |
| **`AssumeRole` via source profile** | `~/.aws/config` with `role_arn = …` + `source_profile = …` | Standard AWS chained-role config |
| **Web-identity (OIDC)** | `AWS_WEB_IDENTITY_TOKEN_FILE` + `AWS_ROLE_ARN` | Set in GitHub Actions / EKS workloads automatically |
| **IMDS** | `http://169.254.169.254/latest/…` on an EC2 instance | Run kleya from inside an EC2 instance with an instance profile attached |

All of the above are tested only at first API call. `kleya` performs no startup credential probe; an invalid or expired token surfaces as a `DescribeKeyPairs` / `RunInstances` / `DescribeVpcs` failure inside whichever subcommand makes the first call.

---

## Profile and region resolution

```
                  CLI                Env                  Config              SDK default
                  ───                ───                  ──────              ───────────
--region <r>  →   KLEYA_REGION  →    config.default_region   →                eu-west-1 (kleya default)
                                                                              ↑
                                                            (AWS_REGION read by aws-config independently)

--profile <p> →   KLEYA_PROFILE →    config.default_profile  →                "default"
                                                                              ↑
                                                            (AWS_PROFILE read by aws-config independently)
```

The CLI resolves to a single `(region, profile)` pair before any adapter call. That pair is what `aws_config::defaults().region(...)` receives. Note that `aws-config` *also* reads `AWS_REGION` / `AWS_PROFILE` from its own environment — kleya does not duplicate that read, but a user-set `AWS_REGION` is observable to the SDK and may surface as the resolved region for credential providers (e.g. STS endpoint selection) even when kleya passes its own `region(...)` override. This is documented as "the SDK is authoritative for SDK-side decisions" and is not considered a divergence; the SDK's own variables are listed in [02-cli-surface.md](02-cli-surface.md#global-flags) for completeness.

`--profile` is forwarded to the SDK only via the `AWS_PROFILE` environment that `aws-config` reads independently; kleya does **not** thread the profile through `aws_config::defaults().profile_name(...)` today. Operators who use `--profile` must set `AWS_PROFILE` in their shell or the CLI flag has no effect on credential resolution. This is a known wart, tracked in Open Questions below.

---

## Browser-based login

The AWS CLI v2 ships two interactive, browser-based login commands. Both are run by the operator outside of kleya; kleya never owns either flow. They are documented here because they are the two flows operators most commonly ask about.

### `aws sso login` (IAM Identity Center)

For organisations using IAM Identity Center. Token consumption works today, with no kleya-side code:

1. Operator runs `aws sso login --profile devbox-sso` once. The AWS CLI v2 writes `~/.aws/sso/cache/<sha1>.json`.
2. Operator runs `kleya launch --profile devbox-sso` (or sets `AWS_PROFILE=devbox-sso`).
3. `aws-config`'s SSO credential provider reads the cache file and the profile's `sso_region` / `sso_account_id` / `sso_role_name` from `~/.aws/config`, exchanges the token for short-lived credentials via STS, and the resulting `Credentials` flow through the default chain to every SDK call kleya makes.

### `aws login` (console credentials, AWS CLI v2 ≥ 2.32.0)

For non-Identity-Center accounts — IAM users, root, or federated IAM identities. The operator runs `aws login --profile <p>`, completes a browser-based authentication against the AWS sign-in console, and the AWS CLI caches refreshable credentials valid for up to 12 hours.

The cache lives at `~/.aws/login/cache/<sha256-of-session-identifier>` (override via `AWS_LOGIN_CACHE_DIRECTORY`). The profile block in `~/.aws/config` carries a `login_session = arn:aws:iam::<account>:user/<name>` line that identifies the session.

`aws-config 1.8.x` ships native support for the `login_session` profile key:

- `aws_config::login::LoginCredentialsProvider` consumes the cached token via the `login::cache` helpers and refreshes it within the session lifetime.
- `aws_config::profile::credentials` parses `login_session = …` into a `BaseProvider::LoginSession { login_session_arn }` variant and the dispatcher constructs a `LoginCredentialsProvider` from it.
- The dispatcher arm is gated `#[cfg(feature = "credentials-login")]`; the same Cargo feature also pulls in `aws-sdk-signin` and the crypto crates the provider needs.

Source references (smithy-rs `adc0a46`): [`aws-config/src/login.rs`](https://github.com/smithy-lang/smithy-rs/blob/adc0a46bad5a34fb77e088377eb294c907e013d8/aws/rust-runtime/aws-config/src/login.rs), [`aws-config/src/profile/credentials/repr.rs`](https://github.com/smithy-lang/smithy-rs/blob/adc0a46bad5a34fb77e088377eb294c907e013d8/aws/rust-runtime/aws-config/src/profile/credentials/repr.rs), [`aws-config/src/profile/credentials/exec.rs`](https://github.com/smithy-lang/smithy-rs/blob/adc0a46bad5a34fb77e088377eb294c907e013d8/aws/rust-runtime/aws-config/src/profile/credentials/exec.rs).

**Current state in kleya:** the workspace `aws-config` dependency is `aws-config = "1"` — default features only (`default-https-client`, `rt-tokio`, `credentials-process`, `sso`). `credentials-login` is **not** in the default set, so `LoginCredentialsProvider` is compiled out and a profile with `login_session = …` produces a `ProfileFileError` from the default chain. Operators relying on `aws login` today have two options:

1. **Wait for the Cargo-feature opt-in.** See Open Questions; the only kleya change needed is one line on each affected `Cargo.toml`. No code change.
2. **Use the `credential_process` workaround documented in the [AWS CLI guide](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sign-in.html#cli-configure-sign-in-cached-credentials)** in the interim:
   ```
   [profile signin]
   login_session = arn:aws:iam::0123456789012:user/dev
   region        = eu-west-1
   [profile kleya]
   credential_process = aws configure export-credentials --profile signin --format process
   region             = eu-west-1
   ```
   `aws login --profile signin` once per session; `kleya --profile kleya` picks up the exported credentials through the `credentials-process` provider that is in the default feature set.

The `credentials-login` feature flag is misleadingly named — it gates both the consumer (`LoginCredentialsProvider`, the thing kleya needs) and the producer (`aws-sdk-signin`, which kleya does **not** wire into any CLI surface). Enabling the feature does not introduce a `kleya login` / `kleya sso login` subcommand; it only lets the SDK's default chain understand a `login_session = …` profile entry.

### Why both flows live outside kleya

The operator does not authenticate through the kleya CLI. Both `aws sso login` and `aws login` are launched from the AWS CLI v2 (or a substitute tool); kleya consumes whatever credentials the SDK's default chain finds. See Decisions below.

---

## Failure surface

| Failure | How it manifests | Operator remediation |
|---|---|---|
| No credentials reachable | First adapter call returns `Error::Adapter { provider: "aws-ec2", source }` whose `Display` includes `dispatch failure: no credential providers...` | Pick a source from the table above and configure it. Run `aws sts get-caller-identity --profile <p>` to verify out-of-band. |
| Browser-login token expired | Same as above, with `source` mentioning `ExpiredToken` or `RefreshFailure` | Re-run `aws sso login --profile <p>` (Identity Center) or `aws login --profile <p>` (console credentials); rerun the kleya command. |
| Profile not declared | `Error::Adapter` with `ProfileNotFound: <name>` | Add the profile to `~/.aws/config` / `~/.aws/credentials`, or use a different `--profile`. |
| AWS clock skew | `Error::Adapter` with `RequestExpired` | Correct system clock — `aws-config` signs requests against local time. (Documented as Assumption in [00-overview.md](00-overview.md#assumptions).) |
| Region rejected by service | `Error::Adapter` with the service's `InvalidRegion` | The kleya `Region` regex (commercial only) catches `cn-*` / `us-gov-*` at config-validation time (exit code 2) before any API call. Other invalid regions slip through to the adapter. |

Credential failures are **never** classified as `ConfigInvalid` — kleya treats authentication state as the SDK's concern. The single exception is the static `Region` validator, which refuses non-commercial regions at `Config::validate()` time before credentials are looked up at all.

---

## What kleya does and does not touch

**Reads (directly):** `~/.config/kleya/config.{toml,yaml,yml,json,jsonc}`, `~/.config/kleya/keys/<name>.pem`, the embedded user-data asset.

**Reads (via `aws-config`, kleya never opens these files):**
- `~/.aws/config`
- `~/.aws/credentials`
- `~/.aws/sso/cache/*.json`
- `~/.aws/cli/cache/*.json` (for cached `AssumeRole` results)
- `~/.aws/login/cache/<sha256>` — read directly by `aws_config::login::LoginCredentialsProvider` **only when the `credentials-login` Cargo feature is enabled** on the workspace `aws-config` dependency (not yet enabled in kleya, see Open Questions). Currently reached only indirectly, via the `aws configure export-credentials` shim invoked by a `credential_process` profile (see "Browser-based login" above).

**Writes (directly):** `~/.config/kleya/keys/<name>.pem` (private), `~/.config/kleya/keys/<name>.pub` (public), and the keys dir itself at mode `0o700`.

**Writes (via `aws-config`):** none. The default feature set (`sso`, `credentials-process`, `rt-tokio`, `default-https-client`) is consumption-only — the SDK reads cached tokens but does not refresh, rotate, or initiate them.

This boundary keeps `aws sso login`, `aws configure`, `aws sts assume-role`, and any equivalent third-party tool (`aws-vault`, `granted`, …) as out-of-process concerns. Operators run `kleya` as a peer to those tools, not a replacement for them.

---

## Limits

- No retry-policy override. The SDK default (`adaptive` retry mode, `aws-config` 1.x defaults: 3 attempts, jittered exponential backoff) governs credential refresh on transient errors. Kleya's named limits in `kleya-core::limits` (`AWS_CALL_TIMEOUT_SECONDS`, `AWS_RETRY_ATTEMPTS_MAX`, …) are not currently wired into `aws-config`; they describe the intended envelope and are reserved for a follow-up that sets `RetryConfig` explicitly. Documented as an Open Question below.
- No per-request signing override. `BehaviorVersion::latest()` is used everywhere; this binds to SigV4 (or the service's published replacement) without operator intervention.

---

## Assumptions and open questions

**Assumptions**

- `aws-config 1.x` default features (`sso`, `credentials-process`, `rt-tokio`, `default-https-client`) are the current AWS-recommended baseline. A future major version that removes `sso` from defaults would be a breaking change documented at upgrade time.
- Operators using IAM Identity Center either run `aws sso login` themselves or set up an automation (`aws-vault`, `granted`) that refreshes the cache. The kleya release binary does not depend on the AWS CLI v2 being installed unless the operator chooses to use SSO.
- Local system clock is within a few minutes of UTC. AWS SigV4 request signing rejects requests more than ~15 minutes skewed.

**Decisions**

- *Authentication happens outside kleya.* **The CLI never owns a login, logout, or auth-driving subcommand.** Operators authenticate using the AWS CLI v2 (`aws sso login` for IAM Identity Center, `aws login` for console credentials, `aws configure` for static keys), `aws-vault`, `granted`, or any equivalent tool of their choice; kleya consumes whatever credentials the SDK's default chain finds. This rule applies to future providers too — when a non-AWS adapter is added, its login flow stays in the provider's own tooling. The rule is about the kleya CLI surface, **not** about the `aws-config` Cargo feature set — enabling `credentials-login` (which adds the `LoginCredentialsProvider` consumer for `login_session` profiles) does not violate this decision, because the consumer reads cached tokens written by `aws login` and never drives the interactive flow itself.
- *No kleya-side credentials store.* **All credential sources live where the SDK already reads from.** Adding a `[credentials]` section in `Config` would duplicate state with `~/.aws/credentials` and `~/.aws/sso/cache/`; the diff between the two would be the inevitable footgun.
- *Profile is resolved to a single string, then handed to the SDK via env.* **kleya does not call `profile_name()` on the `aws-config` loader today.** This means `--profile` only works when `AWS_PROFILE` is also set in the shell — a known wart. The fix is one `.profile_name(profile)` call in `build_ec2_client` and the `SsmClient` builder in `dispatch::run`; deferred to keep the scope of this page descriptive of the current code.
- *No startup credential probe.* **An invalid configuration surfaces at first API call, not at process start.** A pre-flight `sts:GetCallerIdentity` would add a round-trip to every command and would mask the failure under a more generic error; we prefer the precise failure at the call site.
- *Credentials are SDK-internal; no `kleya_core::Error` variant for them.* **`Error::Adapter` is the only public surface for auth failure.** Adding `Error::CredentialsMissing` would invite kleya to interpret SDK error strings (fragile) and would split the exit-code surface for what is logically one adapter failure mode.

**Open questions**

- *Enable the `credentials-login` feature on `aws-config`.* `aws-config 1.8.x` already ships native `LoginCredentialsProvider` plus the profile dispatcher arm for `login_session = …` profiles; both are gated `#[cfg(feature = "credentials-login")]` and that feature is not in the default set. The kleya change is one line on each workspace `Cargo.toml`: `aws-config = { version = "1", features = ["credentials-login"] }`. Trade-offs: adds `aws-sdk-signin` and the RustCrypto deps `p256` / `sha2` / `base64-simd` / `uuid` / `rand` / `zeroize` to the binary (~1 MB on `aarch64-linux`); `cargo deny check` must continue to pass against the expanded tree. No code change beyond the Cargo line, no CLI surface change. Tracked as a small, well-scoped follow-up rather than a Decision because the Cargo.toml change is not yet in the branch this spec describes.
- *Wire `--profile` into `aws-config` directly.* The one-line `.profile_name(profile)` call would make `kleya --profile <p>` work without the operator also setting `AWS_PROFILE`. No design ambiguity; just a fix.
- *Explicit `RetryConfig` matching `kleya_core::limits`.* Today the SDK uses its own defaults, not the named constants. Wiring `RetryConfig::standard().with_max_attempts(AWS_RETRY_ATTEMPTS_MAX).with_initial_backoff(...)` would honor the limits page and make timeouts predictable. Cost is one builder call in `build_ec2_client` plus a small mapping function; deferred for lack of a concrete pain point.
- *MFA / serial-number profiles.* Profiles with `mfa_serial = …` require an interactive token prompt. `aws-config`'s default chain prompts via stdin if a TTY is available; kleya inherits that behaviour but has never been exercised. Confirm before adding MFA to a deployment runbook.
- *Documenting auth-tool recommendations.* CONTRIBUTING / README do not currently recommend a specific external auth tool. If the operator audience converges on one (`aws-vault`, `granted`, …), a non-normative pointer in CONTRIBUTING.md would help; not a kleya code concern.
