# 07 — Error Model

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya` uses one `Error` enum per crate, all derived via `thiserror`. Adapter errors wrap into `kleya_core::Error::Adapter { provider, source }` at the public boundary; `kleya-core` never sees `aws_sdk_ec2::Error` or `aws_sdk_ssm::Error`. `unwrap`, `expect`, `panic!`, `todo!`, and `unimplemented!` are denied via clippy in production code.

---

## Responsibilities

1. Give every recoverable failure a typed variant with a `Display` impl that names the operand (instance id, region, key name).
2. Map every variant to a stable exit code in `kleya-cli`'s `exit_code::code_for`.
3. Translate provider-specific errors at the adapter boundary so the public surface stays provider-neutral.

---

## `kleya_core::Error`

[crates/kleya-core/src/error.rs](../../crates/kleya-core/src/error.rs).

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config invalid: {reason}")]
    ConfigInvalid { reason: String },

    #[error("user-data is too large: {bytes} > {max}")]
    UserDataTooLarge { bytes: usize, max: usize },

    #[error("instance not found: name={name} region={region}")]
    InstanceNotFound { name: String, region: String },

    #[error("ambiguous handle: {name} matches {} instances", .candidates.len())]
    AmbiguousHandle { name: String, candidates: Vec<InstanceId> },

    #[error("ssh not ready after {elapsed_seconds}s for {instance}")]
    SshNotReady { instance: InstanceId, elapsed_seconds: u32 },

    #[error("launch timed out after {elapsed_seconds}s for {instance}")]
    LaunchWaitTimeout { instance: InstanceId, elapsed_seconds: u32 },

    #[error("cancelled: {instance}")]
    Cancelled { instance: InstanceId },

    #[error("ssh key mismatch for {name}: local fingerprint differs from EC2 record")]
    KeyMismatch { name: KeyName },

    #[error("ssh key orphaned: {name} is in EC2 but no local private key")]
    KeyOrphaned { name: KeyName },

    #[error("adapter {provider}: {source}")]
    Adapter {
        provider: &'static str,
        #[source]
        source: BoxError,
    },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub type Result<T, E = Error> = std::result::Result<T, E>;
```

Variant-by-variant intent:

- **`ConfigInvalid`** — covers schema mismatches, regex failures, validation, `cloud-init status --wait` non-zero exits, and operator-confirm aborts. Anything that means "the inputs you gave me aren't acceptable" lands here, with `reason` describing why.
- **`UserDataTooLarge`** — rendered (or operator-supplied) user-data exceeds the raw, gzip, or oversize-input ceiling. Carries both the actual size and the cap so the operator can size their override.
- **`InstanceNotFound`** — no managed instance matches the handle in the active region.
- **`AmbiguousHandle`** — more than one managed instance shares the name. The vector of candidate `InstanceId`s lets the operator pass `--instance-id` to disambiguate.
- **`SshNotReady`** — TCP-22 probe exhausted `SSH_PROBE_TIMEOUT_SECONDS` (default 180 s).
- **`LaunchWaitTimeout`** — state did not reach `Running` within `LAUNCH_WAIT_SECONDS_MAX` (default 600 s) or `max_attempts` (timeout / poll + 2).
- **`Cancelled`** — `CancellationToken` fired during a poll loop. Distinct from `LaunchWaitTimeout` so the operator can tell "I pressed Ctrl-C" from "this is taking too long".
- **`KeyMismatch`** — local fingerprint differs from EC2's. Surfaces the key name; remediation is documented in [README.md](../../README.md#troubleshooting).
- **`KeyOrphaned`** — EC2 has a key with this name but no local pem exists. Operator decides whether to regenerate or import the local public half manually.
- **`Adapter { provider, source }`** — wraps an adapter-specific error. `provider` is currently always `"aws-ec2"`; the `source` is downcast-able to `kleya_aws::AwsError`.
- **`Io`** — wraps `std::io::Error`. Used for filesystem reads, `Command::exec` returning, and the few other places that surface raw IO.

`Error::adapter(provider, source)` is the canonical constructor for `Adapter` from anything `Into<BoxError>`.

---

## `kleya_aws::AwsError`

[crates/kleya-aws/src/error.rs](../../crates/kleya-aws/src/error.rs).

```rust
#[derive(Debug, thiserror::Error)]
pub enum AwsError {
    #[error("ec2 sdk: {0}")]
    Sdk(#[from] BoxError),
    #[error("missing field in response: {0}")]
    MissingField(&'static str),
    #[error("ssm parameter not found: {0}")]
    SsmMissing(String),
}

impl From<AwsError> for kleya_core::Error {
    fn from(e: AwsError) -> Self {
        kleya_core::Error::adapter("aws-ec2", e)
    }
}
```

Adapter-local variants:

- **`Sdk(BoxError)`** — any `aws-sdk-ec2` or `aws-sdk-ssm` error after boxing. The adapter funnels every `*Error` from the SDK through `fn sdk<E>(e: E) -> AwsError` and the `From` impl that wraps it as `kleya_core::Error::Adapter { provider: "aws-ec2", source }`.
- **`MissingField(&'static str)`** — the SDK returned `Some(resp)` but a documented field is `None`. The static string names the field for the operator (e.g., `"launch_template"`, `"group_id"`, `"key_pair after ensure"`).
- **`SsmMissing(String)`** — `GetParameter` returned `Some(parameter)` with `None` value, or the parameter name has no value. Used only by `resolve_ami_alias`.

The `From<AwsError> for kleya_core::Error` impl is what guarantees `kleya-core` callers receive a uniform `Error::Adapter { provider: "aws-ec2", … }` regardless of which internal `AwsError` variant fired.

---

## Exit-code mapping

[crates/kleya-cli/src/exit_code.rs](../../crates/kleya-cli/src/exit_code.rs).

```rust
pub fn code_for(err: &kleya_core::Error) -> i32 {
    match err {
        Error::ConfigInvalid { .. }     => 2,
        Error::InstanceNotFound { .. }  => 3,
        Error::AmbiguousHandle { .. }   => 4,
        Error::SshNotReady { .. }       => 5,
        Error::LaunchWaitTimeout { .. } => 6,
        Error::KeyMismatch { .. } | Error::KeyOrphaned { .. } => 7,
        Error::Cancelled { .. }         => 130,
        Error::Adapter { .. }           => 70,
        Error::Io(_)                    => 74,
        Error::UserDataTooLarge { .. }  => 1,
    }
}
```

Codes 70 and 74 follow `sysexits.h` conventions (`EX_SOFTWARE`, `EX_IOERR`). Code 130 is `128 + SIGINT` — the conventional Unix exit code for a process that exited via Ctrl-C. Clap parse errors exit with code 2 via clap's own error path before `code_for` is invoked.

The mapping is one-to-one for every variant except `KeyMismatch` / `KeyOrphaned`, which share code 7 because the operator remediation is "fix your key-pair state" in both cases.

---

## Display conventions

- Every variant's `#[error("…")]` includes the operand identifier (instance id, key name, region, byte count, elapsed seconds). The user-facing error has enough context to act without a second log line.
- Numeric fields render as decimal without thousands separators.
- The `Adapter` variant's `Display` is `"adapter aws-ec2: <source>"`, so the provider tag is always visible.
- The `AmbiguousHandle` variant's `Display` includes the match count (`matches {count} instances`); the candidate list is on `Debug` but not `Display`. The CLI prints the candidates via `tracing::error!(error = ?e)` when `-v` is set.

---

## Logging the error

[crates/kleya-cli/src/main.rs](../../crates/kleya-cli/src/main.rs) is the only place an `Error` is logged before exit:

```rust
match dispatch::run(cli, cancel).await {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
        tracing::error!(error = %e);
        ExitCode::from(u8::try_from(exit_code::code_for(&e)).unwrap_or(1))
    }
}
```

`%e` uses the `Display` impl; the structured `error` field lands in JSON output as a single string. There is no panic-handler indirection — clippy denies `panic!` in production code so any unhandled panic is a bug, not a runtime concern.

---

## Adapter-boundary wrapping

Every `kleya_aws` public method returns `kleya_core::Result<T>`. The translation pattern is consistent:

```rust
self.ec2.<operation>().<builder>().send().await.map_err(sdk)?
```

`fn sdk<E>(e: E) -> AwsError where E: std::error::Error + Send + Sync + 'static` boxes the SDK error into `AwsError::Sdk(BoxError)`. The `?` on the call site invokes the `From<AwsError> for kleya_core::Error` impl, so the call returns `kleya_core::Error::Adapter { provider: "aws-ec2", source: AwsError::Sdk(…) }`.

Special-case adapter logic (treating `InvalidGroup.Duplicate` / `InvalidKeyPair.Duplicate` / `InvalidPermission.Duplicate` as success) inspects `err.code()` via `ProvideErrorMetadata` *before* wrapping — the duplicate case never surfaces as an `Error::Adapter`.

---

## Assumptions and open questions

**Assumptions**

- `std::io::Error` is the right wrapper for filesystem and `execvp` failures. The conversion is via `#[from]`, so any `Result<_, io::Error>` in `kleya-core` lifts into `kleya_core::Error` with `?`.
- The four `KLEYA_*` env vars never set values that the CLI parses into validated newtypes outside `Config::validate` — i.e., env-supplied region passes the same `Region::new` check as a config-supplied region.
- Operators script against the documented exit codes. Reusing or reassigning a code is a breaking change.

**Decisions**

- *One `Error` enum per crate.* **Adapter-specific variants don't leak into the public surface.** The alternative — a single global `Error` with every provider's failure modes — would couple `kleya-core` to every adapter ever written.
- *Stable exit codes, one per variant (or per remediation class).* **Operators and CI scripts depend on these.** `KeyMismatch` and `KeyOrphaned` share code 7 because the operator remediation is the same; everything else is unique.
- *Cancelled is exit code 130, not 1 or 2.* **Matches Unix convention for SIGINT-terminated processes.** Operators piping kleya through shell loops can `[ $? -eq 130 ] && break` cleanly.
- *No `Error::Internal { reason }` catch-all.* **Every variant names a real failure mode.** A catch-all invites lazy `?` shortcuts where a specific variant would have been more useful.
- *`UserDataTooLarge` is exit code 1, not 2.* **It's neither a config error nor a usage error — it's a content-budget error.** Code 1 is the catch-all "non-success without a more specific code"; carving out 2 for `ConfigInvalid` and reserving 1 for "budget" keeps the higher-numbered codes free for future specific variants.

**Open questions**

- *Variant for `cloud-init status --wait` non-zero.* Resolved (implemented): the `CloudInitFailed` variant exists with exit code 8 (added in the pre-spec-update refactor commit), so operators can script around cloud-init failure distinctly from `ConfigInvalid`.
- *Provider tag on `Adapter`.* Deferred: `provider` remains a `&'static str`. Revisit (and consider constraining to a `Provider::AwsEc2`-style enum) once a second adapter is implemented.
