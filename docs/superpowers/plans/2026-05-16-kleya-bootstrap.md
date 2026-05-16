# Kleya Bootstrap CLI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `kleya`, a Rust CLI that bootstraps AWS spot instances for agentic coding environments, with a port/adapter architecture ready for non-AWS providers.

**Architecture:** Cargo workspace, four crates: `kleya-core` (domain + ports, no I/O), `kleya-aws` (EC2 adapter), `kleya-cli` (the binary), `kleya-bootstrap-assets` (embedded user-data + ghostty terminfo). `kleya-core` defines a `CloudCompute` trait that the AWS adapter implements; the CLI wires everything together. Tests against an `InMemoryCloudCompute` fake for unit/integration, and against Floci (Docker AWS emulator) for AWS-shaped tests.

**Tech Stack:** Rust 1.95.0 (stable), `tokio` (current-thread), `clap`, `serde`, `serde_yaml`, `serde_json`, `toml`, `jsonc-parser`, `minijinja` (user-data template), `flate2` + `base64` (user-data encoding), `aws-sdk-ec2` + `aws-config`, `tracing`, `thiserror`, `async-trait`, `ssh-key` (Ed25519), `md-5` + `hex` (EC2-style fingerprints in the keystore), `nix` (execvp + chmod), `proptest`, `insta` (snapshot tests), `cargo-nextest`, `cargo-llvm-cov`, jj (jujutsu) for VCS, `lefthook` pre-push.

---

## Per-Task Commit Convention

Every task ends with the same VCS sequence after its `Verify` step passes. **Do not skip this** — frequent small commits are mandatory per the Tiger-style guidelines.

```bash
# After Verify passes on Task N:
jj describe -m "<subject>"          # Conventional Commits: type(scope): subject
jj bookmark move main --to @        # advance main to the just-described commit
jj new                              # start Task N+1 in a fresh working revision
# Push only when the user asks; do not push autonomously.
```

**Subject lines per task** (use these verbatim):

| Task | Subject |
|---|---|
| 1  | `chore: scaffold kleya workspace and toolchain` |
| 2  | `feat(core): add named limits with compile-time relations` |
| 3  | `feat(core): add Error enum and Result alias` |
| 4  | `feat(core): add domain newtypes with validation` |
| 5  | `feat(core): add Config struct and validate()` |
| 6  | `feat(core): add CloudCompute, KeyStore, Clock, IdGen ports` |
| 7  | `feat(assets): embed bootstrap template and ghostty terminfo` |
| 8  | `feat(core): render user-data via minijinja` |
| 9  | `feat(core): gzip+base64 encode user-data with size bounds` |
| 10 | `feat(core): add in-memory fakes for test_support` |
| 11 | `feat(core): add template create/update/list/delete service` |
| 12 | `feat(core): add launch service with zero-config defaults` |
| 13 | `feat(core): add list and terminate services` |
| 14 | `feat(core): add connect service with tag-based key lookup` |
| 15 | `feat(aws): scaffold ec2 adapter and client builder` |
| 16 | `feat(aws): implement CloudCompute against aws-sdk-ec2` |
| 17 | `test(aws): integrate Floci-backed adapter tests` |
| 18 | `feat(cli): wire clap surface, dispatch, and exit codes` |
| 19 | `feat(cli): load TOML/YAML/JSON/JSONC config and round-trip` |
| 20 | `feat(cli): filesystem KeyStore with Ed25519 and fingerprint` |
| 21 | `refactor(cli): make dispatch testable via run_with` |
| 22 | `chore: lefthook, cargo-deny, and GitHub Actions CI` |
| 23 | `chore: end-to-end sanity check` |

If a task lands partial work (e.g., a clippy fix that needs a follow-up), use a `wip:` or `fixup:` prefix and squash before pushing to `main`.

---

## File Structure

```
kleya-bootstrap/
  Cargo.toml                                       # workspace
  rust-toolchain.toml
  clippy.toml
  deny.toml
  lefthook.yml
  .gitignore
  .jjignore
  .github/workflows/ci.yml
  crates/
    kleya-core/
      Cargo.toml
      src/lib.rs                                   # crate doc + pub re-exports
      src/error.rs                                 # Error enum
      src/limits.rs                                # named consts + compile-time relations
      src/model/{mod.rs, instance.rs, template.rs, key.rs, tag.rs, market.rs, launch.rs, region.rs}
      src/config.rs                                # Config + validate
      src/ports/{mod.rs, cloud_compute.rs, key_store.rs, clock.rs, id_gen.rs}
      src/bootstrap/{mod.rs, render.rs, encode.rs}
      src/commands/{mod.rs, template.rs, launch.rs, list.rs, connect.rs, terminate.rs}
      src/test_support/{mod.rs, in_memory_compute.rs, in_memory_key_store.rs, fake_clock.rs, fake_id_gen.rs}
      tests/{render_snapshot.rs, config_proptest.rs, commands_with_fakes.rs}
    kleya-aws/
      Cargo.toml
      src/lib.rs
      src/error.rs
      src/client.rs                                # build_client with endpoint override
      src/ec2.rs                                   # CloudCompute impl
      src/mapping.rs                               # core ↔ aws_sdk_ec2 conversions
      tests/floci/{mod.rs, template_lifecycle.rs, instance_lifecycle.rs}
    kleya-cli/
      Cargo.toml
      src/main.rs
      src/clap_args.rs
      src/dispatch.rs
      src/config_loader.rs
      src/key_store_fs.rs
      src/logging.rs
      src/exit_code.rs
      tests/cli_smoke.rs
    kleya-bootstrap-assets/
      Cargo.toml
      src/lib.rs
      assets/setup_devbox.sh.j2
      assets/ghostty.terminfo
  docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md
  docs/superpowers/plans/2026-05-16-kleya-bootstrap.md
```

---

## Task 1: Initialize workspace skeleton and version control

**Files:**
- Create: `/Users/stan/code/kleya-bootstrap/Cargo.toml`
- Create: `/Users/stan/code/kleya-bootstrap/rust-toolchain.toml`
- Create: `/Users/stan/code/kleya-bootstrap/clippy.toml`
- Create: `/Users/stan/code/kleya-bootstrap/.gitignore`
- Create: `/Users/stan/code/kleya-bootstrap/.jjignore`
- Create: empty `crates/{kleya-core,kleya-aws,kleya-cli,kleya-bootstrap-assets}/{Cargo.toml,src/lib.rs}`

- [ ] **Step 1: Pre-flight tool check**

```bash
rustup show active-toolchain || rustup toolchain install stable
cargo --version
cargo nextest --version || cargo install cargo-nextest --locked
cargo llvm-cov --version || cargo install cargo-llvm-cov --locked
jj --version || brew install jj
lefthook version || brew install lefthook
docker --version    # required for Floci tests; non-blocking if missing on dev box
```

Expected: each command prints a version. If `cargo install` runs, expect a 1-2 minute build.

- [ ] **Step 2: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.95.0"
components = ["rustfmt", "clippy", "rust-src"]
profile = "default"
```

- [ ] **Step 3: Write `clippy.toml`**

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
disallowed-methods = [
    { path = "std::result::Result::unwrap",  reason = "use ? or explicit handling" },
    { path = "std::option::Option::unwrap",  reason = "use ? or explicit handling" },
    { path = "std::result::Result::expect",  reason = "use ? or explicit handling" },
    { path = "std::option::Option::expect",  reason = "use ? or explicit handling" },
]
```

- [ ] **Step 4: Write workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/kleya-core",
    "crates/kleya-aws",
    "crates/kleya-cli",
    "crates/kleya-bootstrap-assets",
]

[workspace.package]
edition = "2021"
rust-version = "1.95.0"
license = "MIT OR Apache-2.0"
authors = ["Ant Stanley <ant@senzo.io>"]
repository = "https://github.com/antstanley/kleya"

[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"

[workspace.dependencies]
anyhow = "1"
thiserror = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
toml = "0.8"
jsonc-parser = { version = "0.23", features = ["serde"] }
async-trait = "0.1"
tokio = { version = "1", features = ["macros", "rt", "time", "sync", "net", "process"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
clap = { version = "4", features = ["derive", "env"] }
minijinja = "2"
flate2 = "1"
base64 = "0.22"
regex = "1"
once_cell = "1"
ssh-key = { version = "0.6", features = ["ed25519", "encryption"] }
nix = { version = "0.29", features = ["fs", "user", "process"] }
proptest = "1"
insta = "1"
```

- [ ] **Step 5: Write `.gitignore` and `.jjignore`**

`.gitignore`:
```
/target
**/*.rs.bk
.private/
.envrc
.DS_Store
```

`.jjignore`:
```
/target
.private/
.DS_Store
```

- [ ] **Step 6: Stub each crate**

For each of `kleya-core`, `kleya-aws`, `kleya-cli`, `kleya-bootstrap-assets`, create `crates/<name>/Cargo.toml`:

```toml
[package]
name = "<name>"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[lints]
workspace = true
```

And `crates/<name>/src/lib.rs`:
```rust
//! <one-line description per crate>
```

For `kleya-cli`, additionally make it a binary by adding to its `Cargo.toml`:
```toml
[[bin]]
name = "kleya"
path = "src/main.rs"
```
And `crates/kleya-cli/src/main.rs`:
```rust
fn main() {}
```

- [ ] **Step 7: Verify workspace builds**

```bash
cd /Users/stan/code/kleya-bootstrap
cargo check --workspace
```
Expected: PASS with no warnings (the workspace skeleton compiles).

- [ ] **Step 8: Verify formatter + clippy clean**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: PASS.

- [ ] **Step 9: Commit (jj)** — repo init handled at end of plan; for now no VCS calls.

---

## Task 2: Named limits (`kleya-core/src/limits.rs`)

**Files:**
- Create: `crates/kleya-core/src/limits.rs`
- Modify: `crates/kleya-core/src/lib.rs`
- Test: `crates/kleya-core/src/limits.rs` (in-source `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing test for boundary semantics**

In `crates/kleya-core/src/limits.rs`:
```rust
#![allow(clippy::assertions_on_constants)]

pub const CONFIG_BYTES_MAX: usize            = 256 * 1024;
pub const USER_DATA_RAW_BYTES_MAX: usize     = 16 * 1024;
pub const USER_DATA_ENCODED_BYTES_MAX: usize = 16 * 1024;
pub const TEMPLATES_COUNT_MAX: usize         = 64;
pub const TAGS_PER_TEMPLATE_MAX: usize       = 50;
pub const TAG_KEY_BYTES_MAX: usize           = 128;
pub const TAG_VALUE_BYTES_MAX: usize         = 256;
pub const INSTANCE_NAME_BYTES_MAX: usize     = 63;
pub const KEY_NAME_BYTES_MAX: usize          = 128;
pub const LAUNCH_WAIT_SECONDS_MAX: u32       = 600;
pub const LAUNCH_POLL_INTERVAL_SECONDS: u32  = 5;
pub const SSH_PROBE_PORT: u16                = 22;
pub const SSH_PROBE_TIMEOUT_SECONDS: u32     = 180;
pub const SSH_PROBE_INTERVAL_SECONDS: u32    = 3;
pub const SSH_PROBE_TCP_TIMEOUT_MS: u32      = 2_000;
pub const AWS_CALL_TIMEOUT_SECONDS: u32      = 30;
pub const AWS_RETRY_ATTEMPTS_MAX: u32        = 5;
pub const AWS_RETRY_BACKOFF_BASE_MS: u32     = 200;
pub const AWS_RETRY_BACKOFF_CAP_MS: u32      = 5_000;

const _: () = assert!(LAUNCH_POLL_INTERVAL_SECONDS <= LAUNCH_WAIT_SECONDS_MAX);
const _: () = assert!(SSH_PROBE_INTERVAL_SECONDS <= SSH_PROBE_TIMEOUT_SECONDS);
const _: () = assert!(AWS_RETRY_BACKOFF_BASE_MS <= AWS_RETRY_BACKOFF_CAP_MS);
const _: () = assert!(USER_DATA_RAW_BYTES_MAX <= USER_DATA_ENCODED_BYTES_MAX);
const _: () = assert!(TAG_KEY_BYTES_MAX > 0 && TAG_VALUE_BYTES_MAX > 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_limits_are_aws_compatible() {
        assert_eq!(USER_DATA_RAW_BYTES_MAX, 16_384);
        assert_eq!(USER_DATA_ENCODED_BYTES_MAX, 16_384);
    }

    #[test]
    fn launch_wait_holds_at_least_one_full_interval() {
        assert!(LAUNCH_POLL_INTERVAL_SECONDS > 0);
        assert!(LAUNCH_WAIT_SECONDS_MAX / LAUNCH_POLL_INTERVAL_SECONDS >= 1);
    }
}
```

- [ ] **Step 2: Wire it into `lib.rs`**

In `crates/kleya-core/src/lib.rs`:
```rust
//! kleya-core: domain types, ports, and command orchestration.
//!
//! This crate is free of I/O and provider SDKs. Adapters live in sibling crates.

pub mod limits;
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-core
cargo clippy -p kleya-core --all-targets -- -D warnings
```
Expected: PASS.

---

## Task 3: Error enum (`kleya-core/src/error.rs`)

**Files:**
- Create: `crates/kleya-core/src/error.rs`
- Modify: `crates/kleya-core/src/lib.rs`

- [ ] **Step 1: Write the Error enum and Result alias**

`crates/kleya-core/src/error.rs`:
```rust
use std::fmt;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config invalid: {reason}")]
    ConfigInvalid { reason: String },

    #[error("user-data is too large: {bytes} > {max} bytes")]
    UserDataTooLarge { bytes: usize, max: usize },

    #[error("instance not found: name={name} region={region}")]
    InstanceNotFound { name: String, region: String },

    #[error("ambiguous handle: {name} matches {count} instances")]
    AmbiguousHandle { name: String, count: usize, candidates: Vec<String> },

    #[error("ssh not ready after {elapsed_seconds}s for instance={instance_id}")]
    SshNotReady { instance_id: String, elapsed_seconds: u32 },

    #[error("launch wait timed out after {elapsed_seconds}s for instance={instance_id}")]
    LaunchWaitTimeout { instance_id: String, elapsed_seconds: u32 },

    #[error("ssh key mismatch for {name}: local fingerprint differs from cloud record")]
    KeyMismatch { name: String },

    #[error("ssh key orphaned: {name} is registered with provider but no local private key")]
    KeyOrphaned { name: String },

    #[error("adapter {provider}: {source}")]
    Adapter {
        provider: &'static str,
        #[source]
        source: BoxError,
    },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("template render: {0}")]
    TemplateRender(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    #[must_use]
    pub fn adapter<E>(provider: &'static str, source: E) -> Self
    where
        E: Into<BoxError>,
    {
        Self::Adapter { provider, source: source.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_handle_renders_count() {
        let e = Error::AmbiguousHandle {
            name: "devbox".into(),
            count: 2,
            candidates: vec!["i-1".into(), "i-2".into()],
        };
        let s = format!("{e}");
        assert!(s.contains("devbox"));
        assert!(s.contains('2'));
    }

    #[test]
    fn user_data_too_large_renders_bytes_and_max() {
        let e = Error::UserDataTooLarge { bytes: 17_000, max: 16_384 };
        let s = format!("{e}");
        assert!(s.contains("17000"));
        assert!(s.contains("16384"));
    }
}
```

Add `thiserror` to `crates/kleya-core/Cargo.toml`:
```toml
[dependencies]
thiserror = { workspace = true }
```

- [ ] **Step 2: Export from `lib.rs`**

```rust
pub mod error;
pub mod limits;

pub use error::{Error, Result};
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-core
cargo clippy -p kleya-core --all-targets -- -D warnings
```
Expected: PASS.

---

## Task 4: Domain newtypes — `model::instance`, `model::template`, `model::key`, `model::tag`, `model::region`, `model::market`, `model::launch`

**Files:**
- Create: `crates/kleya-core/src/model/{mod.rs,instance.rs,template.rs,key.rs,tag.rs,region.rs,market.rs,launch.rs}`
- Modify: `crates/kleya-core/src/lib.rs`
- Modify: `crates/kleya-core/Cargo.toml` (add `regex`, `once_cell`, `serde`)

- [ ] **Step 1: Add deps to `crates/kleya-core/Cargo.toml`**

```toml
[dependencies]
thiserror = { workspace = true }
serde     = { workspace = true }
regex     = { workspace = true }
once_cell = { workspace = true }
```

- [ ] **Step 2: Write `model/mod.rs`**

```rust
pub mod instance;
pub mod key;
pub mod launch;
pub mod market;
pub mod region;
pub mod tag;
pub mod template;
```

- [ ] **Step 3: Write `model/instance.rs` (test-first)**

```rust
use crate::error::{Error, Result};
use crate::limits::INSTANCE_NAME_BYTES_MAX;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static INSTANCE_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,62}$").expect("static regex compiles"));

static INSTANCE_ID_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^i-[0-9a-f]{8,32}$").expect("static regex compiles"));

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceName(String);

impl InstanceName {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        assert!(!raw.is_empty(), "InstanceName::new called with empty string");
        if raw.len() > INSTANCE_NAME_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("instance name '{raw}' exceeds {INSTANCE_NAME_BYTES_MAX} bytes"),
            });
        }
        if !INSTANCE_NAME_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("instance name '{raw}' must match ^[a-z0-9][a-z0-9-]{{0,62}}$"),
            });
        }
        assert!(raw.len() <= INSTANCE_NAME_BYTES_MAX);
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(String);

impl InstanceId {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !INSTANCE_ID_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("instance id '{raw}' must match ^i-[0-9a-f]+$"),
            });
        }
        Ok(Self(raw))
    }
    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceState {
    Pending, Running, ShuttingDown, Terminated, Stopping, Stopped, Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    pub name: Option<InstanceName>,
    pub state: InstanceState,
    pub public_dns: Option<String>,
    pub public_ip: Option<String>,
    pub tags: Vec<crate::model::tag::Tag>,
}

#[derive(Debug, Default, Clone)]
pub struct InstanceFilter {
    pub name: Option<String>,
    pub managed_only: bool,
    pub states: Vec<InstanceState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_accepts_simple_lowercase() {
        assert!(InstanceName::new("devbox").is_ok());
        assert!(InstanceName::new("devbox-1").is_ok());
        assert!(InstanceName::new("a").is_ok());
    }

    #[test]
    fn name_rejects_uppercase_and_invalid_chars() {
        assert!(InstanceName::new("DevBox").is_err());
        assert!(InstanceName::new("dev_box").is_err());
        assert!(InstanceName::new("-devbox").is_err());
    }

    #[test]
    fn name_rejects_at_and_above_size_limit() {
        let at_limit = "a".repeat(INSTANCE_NAME_BYTES_MAX);
        let above    = "a".repeat(INSTANCE_NAME_BYTES_MAX + 1);
        assert!(InstanceName::new(&at_limit).is_ok());
        assert!(InstanceName::new(&above).is_err());
    }

    #[test]
    fn id_accepts_canonical_aws_pattern_and_rejects_others() {
        assert!(InstanceId::new("i-0123456789abcdef").is_ok());
        assert!(InstanceId::new("i-deadbeef").is_ok());
        assert!(InstanceId::new("i-").is_err());
        assert!(InstanceId::new("not-an-id").is_err());
    }
}
```

- [ ] **Step 4: Write `model/tag.rs`**

```rust
use crate::error::{Error, Result};
use crate::limits::{TAG_KEY_BYTES_MAX, TAG_VALUE_BYTES_MAX};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag { pub key: String, pub value: String }

impl Tag {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        let key = key.into();
        let value = value.into();
        if key.is_empty() {
            return Err(Error::ConfigInvalid { reason: "tag key empty".into() });
        }
        if key.len() > TAG_KEY_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("tag key '{key}' exceeds {TAG_KEY_BYTES_MAX} bytes"),
            });
        }
        if value.len() > TAG_VALUE_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("tag value for '{key}' exceeds {TAG_VALUE_BYTES_MAX} bytes"),
            });
        }
        Ok(Self { key, value })
    }
}

pub const KLEYA_TAG_MANAGED:  &str = "kleya:managed";
pub const KLEYA_TAG_TEMPLATE: &str = "kleya:template";
pub const KLEYA_TAG_KEY:      &str = "kleya:key";
pub const KLEYA_TAG_NAME:     &str = "Name";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_key() { assert!(Tag::new("", "v").is_err()); }

    #[test]
    fn rejects_oversize_key() {
        let key = "k".repeat(TAG_KEY_BYTES_MAX + 1);
        assert!(Tag::new(key, "v").is_err());
    }

    #[test]
    fn accepts_at_limit() {
        let key = "k".repeat(TAG_KEY_BYTES_MAX);
        let val = "v".repeat(TAG_VALUE_BYTES_MAX);
        assert!(Tag::new(key, val).is_ok());
    }
}
```

- [ ] **Step 5: Write `model/key.rs`**

```rust
use crate::error::{Error, Result};
use crate::limits::KEY_NAME_BYTES_MAX;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static KEY_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z0-9_.-]{1,128}$").expect("static regex compiles"));

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyName(String);

impl KeyName {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(Error::ConfigInvalid { reason: "key name empty".into() });
        }
        if raw.len() > KEY_NAME_BYTES_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("key name '{raw}' exceeds {KEY_NAME_BYTES_MAX} bytes"),
            });
        }
        if !KEY_NAME_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("key name '{raw}' must match ^[A-Za-z0-9_.-]{{1,128}}$"),
            });
        }
        Ok(Self(raw))
    }
    #[must_use] pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for KeyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct PublicKey(pub String);   // OpenSSH-format text

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub name: KeyName,
    pub public:  PublicKey,
    pub private: String,            // OpenSSH-format text
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(pub String); // hex md5 in EC2-style: "aa:bb:..."

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn accepts_valid_names() {
        assert!(KeyName::new("devbox").is_ok());
        assert!(KeyName::new("kleya-default").is_ok());
        assert!(KeyName::new("a.b_c-1").is_ok());
    }

    #[test] fn rejects_path_traversal_chars() {
        assert!(KeyName::new("../oops").is_err());
        assert!(KeyName::new("foo/bar").is_err());
        assert!(KeyName::new("foo bar").is_err());
    }
}
```

- [ ] **Step 6: Write `model/region.rs`, `model/market.rs`, `model/template.rs`, `model/launch.rs`**

`region.rs`:
```rust
use crate::error::{Error, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static REGION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z]{2}-[a-z]+-[0-9]+$").expect("static regex compiles"));

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Region(String);
impl Region {
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        if !REGION_RE.is_match(&raw) {
            return Err(Error::ConfigInvalid {
                reason: format!("region '{raw}' invalid (e.g. eu-west-1)"),
            });
        }
        Ok(Self(raw))
    }
    #[must_use] pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AmiId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubnetId(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecurityGroupId(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn accepts_eu_west_1() { assert!(Region::new("eu-west-1").is_ok()); }
    #[test] fn rejects_garbage()    { assert!(Region::new("eu west 1").is_err()); }
}
```

`market.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarketKind { Spot, OnDemand }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpotType { OneTime, Persistent }
```

`template.rs`:
```rust
use serde::{Deserialize, Serialize};
use crate::model::{key::KeyName, market::{MarketKind, SpotType},
                   region::{AmiId, SecurityGroupId, SubnetId}, tag::Tag};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateName(pub String);

#[derive(Debug, Clone)]
pub struct TemplateSpec {
    pub name: TemplateName,
    pub ami_id: Option<AmiId>,
    pub ami_alias: Option<String>,
    pub instance_type: String,
    pub key_name: KeyName,
    pub security_group_ids: Vec<SecurityGroupId>,
    pub subnet_id: Option<SubnetId>,
    pub market: MarketKind,
    pub spot_type: SpotType,
    pub tags: Vec<Tag>,
    pub user_data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVersion(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: TemplateId,
    pub name: TemplateName,
    pub latest_version: TemplateVersion,
}
```

`launch.rs`:
```rust
use std::time::Duration;
use crate::model::{instance::InstanceName, key::KeyName, market::{MarketKind, SpotType},
                   tag::Tag, template::TemplateName};

#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub template: TemplateName,
    pub instance_name: InstanceName,
    pub instance_type_override: Option<String>,
    pub market_override: Option<MarketKind>,
    pub spot_type_override: Option<SpotType>,
    pub extra_tags: Vec<Tag>,
    pub key_name: KeyName,
}

#[derive(Debug, Clone, Copy)]
pub struct Deadline { pub timeout: Duration, pub poll_interval: Duration }
```

- [ ] **Step 7: Wire model into `lib.rs`**

```rust
pub mod error;
pub mod limits;
pub mod model;
pub use error::{Error, Result};
```

- [ ] **Step 8: Verify**

```bash
cargo nextest run -p kleya-core
cargo clippy -p kleya-core --all-targets -- -D warnings
```
Expected: PASS, no warnings.

---

## Task 5: Config + multi-format compatible struct (`kleya-core/src/config.rs`)

**Files:**
- Create: `crates/kleya-core/src/config.rs`
- Modify: `crates/kleya-core/src/lib.rs`

- [ ] **Step 1: Write `Config` and `validate`**

```rust
use serde::{Deserialize, Serialize};
use crate::error::{Error, Result};
use crate::limits::{TEMPLATES_COUNT_MAX, TAGS_PER_TEMPLATE_MAX};
use crate::model::{key::KeyName, region::Region, tag::Tag};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_region")] pub default_region: String,
    #[serde(default = "default_profile")] pub default_profile: String,
    #[serde(default)] pub defaults:  Defaults,
    #[serde(default)] pub bootstrap: BootstrapCfg,
    #[serde(default)] pub ssh:       SshCfg,
    #[serde(default)] pub keys:      KeysCfg,
    #[serde(default)] pub templates: Vec<TemplateCfg>,
}

fn default_region()  -> String { "eu-west-1".into() }
fn default_profile() -> String { "default".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "default_instance_type")] pub instance_type: String,
    #[serde(default = "default_market")]        pub market:        String,
    #[serde(default = "default_spot_type")]     pub spot_type:     String,
    #[serde(default = "default_ami_alias")]     pub ami_alias:     String,
}
fn default_instance_type() -> String { "m8g.xlarge".into() }
fn default_market()        -> String { "spot".into() }
fn default_spot_type()     -> String { "one-time".into() }
fn default_ami_alias()     -> String { "amazon-linux-2023-arm64".into() }
impl Default for Defaults {
    fn default() -> Self { Self {
        instance_type: default_instance_type(),
        market:        default_market(),
        spot_type:     default_spot_type(),
        ami_alias:     default_ami_alias(),
    }}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BootstrapCfg {
    #[serde(default)] pub user_data_path: Option<String>,
    #[serde(default = "yes")] pub install_ghostty_terminfo: bool,
}
fn yes() -> bool { true }
impl Default for BootstrapCfg {
    fn default() -> Self { Self { user_data_path: None, install_ghostty_terminfo: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SshCfg {
    #[serde(default = "default_ssh_user")] pub user: String,
    #[serde(default = "yes")]              pub tmux: bool,
    #[serde(default = "default_session")]  pub tmux_session: String,
    #[serde(default)]                      pub extra_args: Vec<String>,
}
fn default_ssh_user() -> String { "ec2-user".into() }
fn default_session()  -> String { "kleya".into() }
impl Default for SshCfg {
    fn default() -> Self { Self {
        user: default_ssh_user(), tmux: true, tmux_session: default_session(), extra_args: vec![],
    }}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeysCfg {
    #[serde(default = "default_keys_dir")]   pub dir: String,
    #[serde(default = "default_key_name")]   pub default_key_name: String,
}
fn default_keys_dir() -> String { "~/.config/kleya/keys".into() }
fn default_key_name() -> String { "kleya-default".into() }
impl Default for KeysCfg {
    fn default() -> Self { Self { dir: default_keys_dir(), default_key_name: default_key_name() }}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TemplateCfg {
    pub name: String,
    pub ami_id: Option<String>,
    pub instance_type: Option<String>,
    pub key_name: Option<String>,
    pub security_group_ids: Option<Vec<String>>,
    pub subnet_id: Option<String>,
    #[serde(default)] pub tags: Vec<TagCfg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TagCfg { pub key: String, pub value: String }

impl Default for Config {
    fn default() -> Self { Self {
        default_region:  default_region(),
        default_profile: default_profile(),
        defaults:  Defaults::default(),
        bootstrap: BootstrapCfg::default(),
        ssh:       SshCfg::default(),
        keys:      KeysCfg::default(),
        templates: vec![],
    }}
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        Region::new(&self.default_region)?;
        if self.templates.len() > TEMPLATES_COUNT_MAX {
            return Err(Error::ConfigInvalid {
                reason: format!("too many templates: {} > {TEMPLATES_COUNT_MAX}", self.templates.len()),
            });
        }
        for t in &self.templates {
            if let Some(k) = &t.key_name { KeyName::new(k)?; }
            if t.tags.len() > TAGS_PER_TEMPLATE_MAX {
                return Err(Error::ConfigInvalid {
                    reason: format!("template '{}' has {} tags > {TAGS_PER_TEMPLATE_MAX}",
                                    t.name, t.tags.len()),
                });
            }
            for tag in &t.tags { Tag::new(&tag.key, &tag.value)?; }
        }
        match self.defaults.market.as_str() {
            "spot" | "on-demand" => {}
            other => return Err(Error::ConfigInvalid {
                reason: format!("defaults.market must be spot|on-demand (got '{other}')"),
            }),
        }
        match self.defaults.spot_type.as_str() {
            "one-time" | "persistent" => {}
            other => return Err(Error::ConfigInvalid {
                reason: format!("defaults.spot_type must be one-time|persistent (got '{other}')"),
            }),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn defaults_validate() {
        Config::default().validate().expect("defaults are valid");
    }

    #[test] fn rejects_bad_region() {
        let mut c = Config::default();
        c.default_region = "not a region".into();
        assert!(c.validate().is_err());
    }

    #[test] fn rejects_excess_templates() {
        let mut c = Config::default();
        c.templates = (0..(TEMPLATES_COUNT_MAX + 1))
            .map(|i| TemplateCfg {
                name: format!("t{i}"), ami_id: None, instance_type: None, key_name: None,
                security_group_ids: None, subnet_id: None, tags: vec![],
            })
            .collect();
        assert!(c.validate().is_err());
    }

    #[test] fn rejects_unknown_market() {
        let mut c = Config::default();
        c.defaults.market = "lottery".into();
        assert!(c.validate().is_err());
    }
}
```

- [ ] **Step 2: Wire into `lib.rs`**

```rust
pub mod config;
pub mod error;
pub mod limits;
pub mod model;
pub use config::Config;
pub use error::{Error, Result};
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 6: Ports — `CloudCompute`, `KeyStore`, `Clock`, `IdGen`

**Files:**
- Create: `crates/kleya-core/src/ports/{mod.rs, cloud_compute.rs, key_store.rs, clock.rs, id_gen.rs}`
- Modify: `crates/kleya-core/src/lib.rs`
- Modify: `crates/kleya-core/Cargo.toml` (add `async-trait`)

- [ ] **Step 1: Add `async-trait` dep**

```toml
async-trait = { workspace = true }
```

- [ ] **Step 2: Write `ports/mod.rs`**

```rust
pub mod clock;
pub mod cloud_compute;
pub mod id_gen;
pub mod key_store;
```

- [ ] **Step 3: Write `ports/cloud_compute.rs`**

```rust
use async_trait::async_trait;
use crate::Result;
use crate::model::{
    instance::{Instance, InstanceFilter, InstanceId},
    key::{KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    region::{AmiId, SecurityGroupId, SubnetId},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};

#[async_trait]
pub trait CloudCompute: Send + Sync {
    async fn template_create (&self, spec: &TemplateSpec) -> Result<TemplateId>;
    async fn template_update (&self, id: &TemplateId, spec: &TemplateSpec) -> Result<TemplateVersion>;
    async fn template_list   (&self) -> Result<Vec<TemplateSummary>>;
    async fn template_delete (&self, id: &TemplateId) -> Result<()>;
    async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>>;

    async fn instance_launch       (&self, req: &LaunchRequest) -> Result<Instance>;
    async fn instance_list         (&self, filter: &InstanceFilter) -> Result<Vec<Instance>>;
    async fn instance_describe     (&self, id: &InstanceId) -> Result<Instance>;
    async fn instance_terminate    (&self, id: &InstanceId) -> Result<()>;
    async fn instance_wait_running (&self, id: &InstanceId, deadline: Deadline) -> Result<Instance>;

    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId>;
    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()>;
    async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<String>>;
    async fn resolve_default_subnet(&self) -> Result<SubnetId>;
    async fn resolve_ami_alias(&self, alias: &str) -> Result<AmiId>;
}
```

- [ ] **Step 4: Write `ports/key_store.rs`**

```rust
use std::path::PathBuf;
use crate::Result;
use crate::model::key::{Fingerprint, KeyName, KeyPair, PublicKey};

pub trait KeyStore: Send + Sync {
    fn ensure_dir(&self) -> Result<PathBuf>;
    fn generate(&self, name: &KeyName) -> Result<KeyPair>;
    fn read_public(&self, name: &KeyName) -> Result<PublicKey>;
    fn private_path(&self, name: &KeyName) -> Result<PathBuf>;
    fn exists(&self, name: &KeyName) -> bool;
    fn delete(&self, name: &KeyName) -> Result<()>;

    /// EC2-style MD5 fingerprint of the *public* key, colon-separated hex.
    /// Format: `aa:bb:cc:...` over the MD5 of the base64-decoded body of the
    /// OpenSSH public-key line. Matches what AWS returns from
    /// `DescribeKeyPairs` for imported keys.
    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint>;
}
```

- [ ] **Step 5: Write `ports/clock.rs` and `ports/id_gen.rs`**

`clock.rs`:
```rust
use std::time::{Duration, Instant};

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, dur: Duration);   // blocking; async wrappers handle in adapter
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant { Instant::now() }
    fn sleep(&self, dur: Duration) { std::thread::sleep(dur); }
}
```

`id_gen.rs`:
```rust
pub trait IdGen: Send + Sync {
    fn name(&self) -> String;
}

pub struct AdjAnimalIdGen;
impl IdGen for AdjAnimalIdGen {
    fn name(&self) -> String {
        const ADJ:    &[&str] = &["brave","calm","eager","fancy","gentle","happy","jolly","keen",
                                  "lucky","merry","nifty","proud","quick","red","sunny","witty"];
        const ANIMAL: &[&str] = &["otter","fox","tiger","hawk","lynx","wolf","crow","badger",
                                  "puma","seal","kite","owl","whale","squid","heron","koi"];
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos()).unwrap_or(0);
        let a = ADJ[(nanos as usize)        % ADJ.len()];
        let b = ANIMAL[(nanos as usize / 7) % ANIMAL.len()];
        format!("kleya-{a}-{b}")
    }
}
```

- [ ] **Step 6: Wire into `lib.rs`**

```rust
pub mod bootstrap;       // empty for now; created in next task
pub mod commands;        // empty for now
pub mod config;
pub mod error;
pub mod limits;
pub mod model;
pub mod ports;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use config::Config;
pub use error::{Error, Result};
```

Create empty placeholder modules to keep compile green:
- `crates/kleya-core/src/bootstrap/mod.rs`: `pub mod render; pub mod encode;`
- `crates/kleya-core/src/bootstrap/render.rs`: `// filled in Task 8`
- `crates/kleya-core/src/bootstrap/encode.rs`: `// filled in Task 9`
- `crates/kleya-core/src/commands/mod.rs`: `// filled in Task 11+`
- `crates/kleya-core/src/test_support/mod.rs`: `// filled in Task 10`

For each truly empty module file, write just `// placeholder` so they compile.

Add the `test-support` feature to `Cargo.toml`:
```toml
[features]
default = []
test-support = []
```

- [ ] **Step 7: Verify**

```bash
cargo check -p kleya-core
cargo clippy -p kleya-core --all-targets -- -D warnings
```
Expected: PASS.

---

## Task 7: Bootstrap assets crate (`kleya-bootstrap-assets`)

**Files:**
- Modify: `crates/kleya-bootstrap-assets/Cargo.toml`
- Create: `crates/kleya-bootstrap-assets/assets/setup_devbox.sh.j2`
- Create: `crates/kleya-bootstrap-assets/assets/ghostty.terminfo`
- Modify: `crates/kleya-bootstrap-assets/src/lib.rs`

- [ ] **Step 1: Write `setup_devbox.sh.j2`**

```
#!/usr/bin/env bash
set -euxo pipefail

sudo dnf update -y
sudo dnf install -y git zsh util-linux-user ncurses

# zsh + oh-my-zsh
sudo chsh -s "$(which zsh)" "$(whoami)"
sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended
git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting
git clone https://github.com/zsh-users/zsh-autosuggestions   ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-autosuggestions
sed -i 's/(git)/(git zsh-autosuggestions zsh-syntax-highlighting)/g' ~/.zshrc

sudo dnf config-manager --add-repo https://cli.github.com/packages/rpm/gh-cli.repo
sudo dnf install -y gh tmux

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
echo 'export PATH="$PATH:$HOME/.cargo/bin"' >> ~/.zshrc

curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.4/install.sh | bash
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
nvm install {{ node_major }}

curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
"$HOME/.cargo/bin/cargo" binstall -y --disable-telemetry --strategies crate-meta-data jj-cli

sudo dnf install -y python{{ python_version }} python{{ python_version }}-pip
echo 'alias python=python{{ python_version }}' >> ~/.zshrc
echo 'alias pip=pip{{ python_version }}' >> ~/.zshrc
curl -LsSf https://astral.sh/uv/install.sh | sh

{% if install_dev_tools -%}
sudo dnf groupinstall -y "Development Tools"
{%- endif %}

curl -fsSL https://claude.ai/install.sh | bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc

{% if install_ghostty_terminfo -%}
# ghostty terminfo (server-side install)
cat > /tmp/ghostty.terminfo <<'GHOSTTY_TERMINFO_EOF'
{{ ghostty_terminfo_source }}
GHOSTTY_TERMINFO_EOF
sudo tic -x /tmp/ghostty.terminfo
rm /tmp/ghostty.terminfo
{%- endif %}

{% for line in extra_pre_lines -%}
{{ line }}
{% endfor -%}
{% for line in extra_post_lines -%}
{{ line }}
{% endfor -%}
```

- [ ] **Step 2: Fetch and pin `ghostty.terminfo`**

```bash
curl -fsSL https://raw.githubusercontent.com/ghostty-org/ghostty/main/include/ghostty.terminfo \
    -o /Users/stan/code/kleya-bootstrap/crates/kleya-bootstrap-assets/assets/ghostty.terminfo
# Prepend a SHA-pin comment for traceability
SHA=$(curl -fsSL "https://api.github.com/repos/ghostty-org/ghostty/commits/main?path=include/ghostty.terminfo" | \
      grep -m1 '"sha"' | cut -d'"' -f4)
sed -i.bak "1s|^|# ghostty-org/ghostty@${SHA} (pinned $(date -u +%F))\n|" \
    /Users/stan/code/kleya-bootstrap/crates/kleya-bootstrap-assets/assets/ghostty.terminfo
rm /Users/stan/code/kleya-bootstrap/crates/kleya-bootstrap-assets/assets/ghostty.terminfo.bak
```

Expected: file is present, first line is a `# ghostty-org/ghostty@...` comment.

- [ ] **Step 3: Wire into `lib.rs`**

```rust
//! Embedded bootstrap script + ghostty terminfo source.

pub const SETUP_TEMPLATE: &str =
    include_str!("../assets/setup_devbox.sh.j2");

pub const GHOSTTY_TERMINFO: &str =
    include_str!("../assets/ghostty.terminfo");

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn assets_are_non_empty() {
        assert!(!SETUP_TEMPLATE.is_empty());
        assert!(!GHOSTTY_TERMINFO.is_empty());
    }
}
```

- [ ] **Step 4: Verify**

```bash
cargo nextest run -p kleya-bootstrap-assets
```
Expected: PASS.

---

## Task 8: Bootstrap render (`kleya-core/src/bootstrap/render.rs`)

**Files:**
- Modify: `crates/kleya-core/src/bootstrap/render.rs`
- Modify: `crates/kleya-core/Cargo.toml` (add `minijinja`)
- Modify: `crates/kleya-core/src/bootstrap/mod.rs`

- [ ] **Step 1: Add `minijinja` dep**

```toml
minijinja = { workspace = true }
```

- [ ] **Step 2: Write `bootstrap/render.rs`**

```rust
use crate::error::{Error, Result};
use minijinja::{Environment, context};

pub struct BootstrapVars<'a> {
    pub install_ghostty_terminfo: bool,
    pub ghostty_terminfo_source: &'a str,
    pub install_dev_tools: bool,
    pub node_major: u8,
    pub python_version: &'a str,
    pub extra_pre_lines:  &'a [String],
    pub extra_post_lines: &'a [String],
}

impl<'a> BootstrapVars<'a> {
    #[must_use] pub fn default_with(
        ghostty_terminfo_source: &'a str,
    ) -> Self { Self {
        install_ghostty_terminfo: true,
        ghostty_terminfo_source,
        install_dev_tools: false,
        node_major: 24,
        python_version: "3.14",
        extra_pre_lines:  &[],
        extra_post_lines: &[],
    }}
}

pub fn render(template: &str, vars: &BootstrapVars<'_>) -> Result<String> {
    assert!(!template.is_empty(), "bootstrap template empty");
    assert!(vars.node_major >= 18, "node_major too low");
    let mut env = Environment::new();
    env.add_template("setup", template).map_err(|e| Error::TemplateRender(e.to_string()))?;
    let tpl = env.get_template("setup").map_err(|e| Error::TemplateRender(e.to_string()))?;
    let out = tpl.render(context! {
        install_ghostty_terminfo => vars.install_ghostty_terminfo,
        ghostty_terminfo_source  => vars.ghostty_terminfo_source,
        install_dev_tools        => vars.install_dev_tools,
        node_major               => vars.node_major,
        python_version           => vars.python_version,
        extra_pre_lines          => vars.extra_pre_lines,
        extra_post_lines         => vars.extra_post_lines,
    }).map_err(|e| Error::TemplateRender(e.to_string()))?;
    assert!(!out.is_empty(), "rendered output empty");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "set -e\n\
                     {% if install_ghostty_terminfo %}GHOSTTY{% endif %}\n\
                     node{{ node_major }} py{{ python_version }}\n";

    #[test] fn renders_with_ghostty_block() {
        let v = BootstrapVars {
            install_ghostty_terminfo: true,
            ghostty_terminfo_source: "TERMINFO",
            install_dev_tools: false,
            node_major: 24,
            python_version: "3.14",
            extra_pre_lines: &[], extra_post_lines: &[],
        };
        let out = render(T, &v).expect("renders");
        assert!(out.contains("GHOSTTY"));
        assert!(out.contains("node24"));
        assert!(out.contains("py3.14"));
    }

    #[test] fn omits_ghostty_block_when_disabled() {
        let v = BootstrapVars {
            install_ghostty_terminfo: false,
            ghostty_terminfo_source: "",
            install_dev_tools: false,
            node_major: 24,
            python_version: "3.14",
            extra_pre_lines: &[], extra_post_lines: &[],
        };
        let out = render(T, &v).expect("renders");
        assert!(!out.contains("GHOSTTY"));
    }
}
```

- [ ] **Step 3: Snapshot test against the real template (in `tests/render_snapshot.rs`)**

Add `insta` dev-dep to `crates/kleya-core/Cargo.toml`:
```toml
[dev-dependencies]
insta = { workspace = true, features = ["yaml"] }
```

Create `crates/kleya-core/tests/render_snapshot.rs`:
```rust
use kleya_core::bootstrap::render::{render, BootstrapVars};

#[test]
fn renders_real_template_with_ghostty() {
    let template = kleya_bootstrap_assets::SETUP_TEMPLATE;
    let ghostty  = kleya_bootstrap_assets::GHOSTTY_TERMINFO;
    let vars = BootstrapVars::default_with(ghostty);
    let out = render(template, &vars).expect("renders");
    insta::assert_snapshot!("setup_devbox_default", out);
}

#[test]
fn renders_real_template_without_ghostty() {
    let template = kleya_bootstrap_assets::SETUP_TEMPLATE;
    let mut vars = BootstrapVars::default_with("");
    vars.install_ghostty_terminfo = false;
    let out = render(template, &vars).expect("renders");
    insta::assert_snapshot!("setup_devbox_no_ghostty", out);
}
```

Add the assets crate as a dev-dep:
```toml
kleya-bootstrap-assets = { path = "../kleya-bootstrap-assets" }
```

- [ ] **Step 4: Verify and accept snapshots**

```bash
cargo nextest run -p kleya-core
```

First run will fail with "snapshot file not found, generated `.snap.new`." Open each `.snap.new` and **read it** — confirm the rendered bash script looks right (zsh setup, ghostty terminfo block present/absent as expected, no template syntax left unrendered). Only after manual review:

```bash
cargo insta accept --workspace        # or rename .snap.new → .snap by hand
cargo nextest run -p kleya-core       # now passes
```

Do NOT use `INSTA_UPDATE=always` in CI or on subsequent runs — it bypasses the review step and lets template regressions land silently.

Expected: PASS after manual snapshot review.

---

## Task 9: Bootstrap encode (`kleya-core/src/bootstrap/encode.rs`)

**Files:**
- Modify: `crates/kleya-core/src/bootstrap/encode.rs`
- Modify: `crates/kleya-core/Cargo.toml` (add `flate2`, `base64`)

- [ ] **Step 1: Add deps**

```toml
flate2 = { workspace = true }
base64 = { workspace = true }
```

- [ ] **Step 2: Write `bootstrap/encode.rs`**

```rust
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write as _;

use crate::error::{Error, Result};
use crate::limits::{USER_DATA_ENCODED_BYTES_MAX, USER_DATA_RAW_BYTES_MAX};

pub fn encode_user_data(raw: &str) -> Result<String> {
    assert!(!raw.is_empty(), "encode_user_data called with empty raw");
    let raw_bytes = raw.len();
    if raw_bytes > USER_DATA_RAW_BYTES_MAX {
        return Err(Error::UserDataTooLarge { bytes: raw_bytes, max: USER_DATA_RAW_BYTES_MAX });
    }
    let mut enc = GzEncoder::new(Vec::with_capacity(raw_bytes), Compression::best());
    enc.write_all(raw.as_bytes())?;
    let gz = enc.finish()?;
    let b64 = B64.encode(&gz);
    if b64.len() > USER_DATA_ENCODED_BYTES_MAX {
        return Err(Error::UserDataTooLarge { bytes: b64.len(), max: USER_DATA_ENCODED_BYTES_MAX });
    }
    assert!(!b64.is_empty(), "encoded output empty");
    Ok(b64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn encodes_small_script() {
        let s = encode_user_data("#!/usr/bin/env bash\necho hi\n").expect("encodes");
        assert!(!s.is_empty());
    }

    #[test] fn rejects_oversize_raw_input() {
        let big = "x".repeat(USER_DATA_RAW_BYTES_MAX + 1);
        let err = encode_user_data(&big).unwrap_err();
        assert!(matches!(err, Error::UserDataTooLarge { .. }));
    }

    #[test] fn boundary_at_raw_max_succeeds() {
        let at = "x".repeat(USER_DATA_RAW_BYTES_MAX);
        assert!(encode_user_data(&at).is_ok());
    }
}
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 10: In-memory fakes (`kleya-core/src/test_support/`)

**Files:**
- Modify: `crates/kleya-core/src/test_support/mod.rs`
- Create: `crates/kleya-core/src/test_support/{in_memory_compute.rs, in_memory_key_store.rs, fake_clock.rs, fake_id_gen.rs}`

- [ ] **Step 1: Wire `test_support/mod.rs`**

```rust
//! Fakes for testing kleya-core commands without a real provider.
pub mod fake_clock;
pub mod fake_id_gen;
pub mod in_memory_compute;
pub mod in_memory_key_store;

pub use fake_clock::FakeClock;
pub use fake_id_gen::FakeIdGen;
pub use in_memory_compute::InMemoryCompute;
pub use in_memory_key_store::InMemoryKeyStore;
```

- [ ] **Step 2: Write `fake_id_gen.rs`**

```rust
use std::sync::Mutex;
use crate::ports::id_gen::IdGen;

pub struct FakeIdGen { next: Mutex<u64> }

impl FakeIdGen {
    #[must_use] pub fn new() -> Self { Self { next: Mutex::new(0) }}
}

impl Default for FakeIdGen { fn default() -> Self { Self::new() }}

impl IdGen for FakeIdGen {
    fn name(&self) -> String {
        let mut g = self.next.lock().expect("mutex");
        let v = *g;
        *g += 1;
        format!("kleya-test-{v:04}")
    }
}
```

- [ ] **Step 3: Write `fake_clock.rs`**

```rust
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::ports::clock::Clock;

pub struct FakeClock {
    state: Mutex<(Instant, Vec<Duration>)>, // (now, slept)
}

impl FakeClock {
    #[must_use] pub fn new() -> Self { Self { state: Mutex::new((Instant::now(), vec![])) }}
    pub fn advance(&self, by: Duration) {
        let mut s = self.state.lock().expect("mutex");
        s.0 += by;
    }
    #[must_use] pub fn slept(&self) -> Vec<Duration> {
        self.state.lock().expect("mutex").1.clone()
    }
}

impl Default for FakeClock { fn default() -> Self { Self::new() }}

impl Clock for FakeClock {
    fn now(&self) -> Instant { self.state.lock().expect("mutex").0 }
    fn sleep(&self, dur: Duration) {
        let mut s = self.state.lock().expect("mutex");
        s.0 += dur;
        s.1.push(dur);
    }
}
```

- [ ] **Step 4: Write `in_memory_key_store.rs`**

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::Result;
use crate::error::Error;
use crate::model::key::{Fingerprint, KeyName, KeyPair, PublicKey};
use crate::ports::key_store::KeyStore;

pub struct InMemoryKeyStore {
    keys: Mutex<HashMap<KeyName, KeyPair>>,
}

impl InMemoryKeyStore {
    #[must_use] pub fn new() -> Self { Self { keys: Mutex::new(HashMap::new()) }}
}
impl Default for InMemoryKeyStore { fn default() -> Self { Self::new() }}

impl KeyStore for InMemoryKeyStore {
    fn ensure_dir(&self) -> Result<PathBuf> { Ok(PathBuf::from("/in-memory")) }

    fn generate(&self, name: &KeyName) -> Result<KeyPair> {
        let pair = KeyPair {
            name: name.clone(),
            public:  PublicKey(format!("ssh-ed25519 FAKE {name}")),
            private: format!("-----BEGIN FAKE KEY-----\n{name}\n-----END FAKE KEY-----\n"),
        };
        self.keys.lock().expect("mutex").insert(name.clone(), pair.clone());
        Ok(pair)
    }

    fn read_public(&self, name: &KeyName) -> Result<PublicKey> {
        self.keys.lock().expect("mutex").get(name)
            .map(|kp| kp.public.clone())
            .ok_or_else(|| Error::KeyOrphaned { name: name.to_string() })
    }

    fn private_path(&self, name: &KeyName) -> Result<PathBuf> {
        if !self.exists(name) {
            return Err(Error::KeyOrphaned { name: name.to_string() });
        }
        Ok(PathBuf::from(format!("/in-memory/{name}.pem")))
    }

    fn exists(&self, name: &KeyName) -> bool {
        self.keys.lock().expect("mutex").contains_key(name)
    }

    fn delete(&self, name: &KeyName) -> Result<()> {
        self.keys.lock().expect("mutex").remove(name);
        Ok(())
    }

    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint> {
        let pub_text = self.read_public(name)?;
        // In-memory fake: deterministic hash of the public text, not a real MD5.
        // Real implementation lives in FsKeyStore.
        let h = crc32fast::hash(pub_text.0.as_bytes());
        Ok(Fingerprint(format!("fake:{h:08x}")))
    }
}
```

- [ ] **Step 5: Write `in_memory_compute.rs`**

```rust
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::Result;
use crate::error::Error;
use crate::model::{
    instance::{Instance, InstanceFilter, InstanceId, InstanceState, InstanceName},
    key::{KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    region::{AmiId, SecurityGroupId, SubnetId},
    tag::{Tag, KLEYA_TAG_KEY, KLEYA_TAG_MANAGED, KLEYA_TAG_NAME, KLEYA_TAG_TEMPLATE},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};
use crate::ports::cloud_compute::CloudCompute;

#[derive(Default)]
struct State {
    templates:  HashMap<TemplateName, (TemplateId, TemplateSpec, TemplateVersion)>,
    instances:  HashMap<InstanceId, Instance>,
    sgs:        HashMap<String, SecurityGroupId>,
    keypairs:   HashMap<KeyName, String /* fingerprint */>,
    next_id:    u64,
}

pub struct InMemoryCompute {
    state: Mutex<State>,
    default_subnet: SubnetId,
    default_ami:    AmiId,
}

impl InMemoryCompute {
    #[must_use]
    pub fn new() -> Self { Self {
        state: Mutex::new(State::default()),
        default_subnet: SubnetId("subnet-fake".into()),
        default_ami:    AmiId("ami-fake".into()),
    }}

    fn next_instance_id(&self) -> InstanceId {
        let mut s = self.state.lock().expect("mutex");
        s.next_id += 1;
        InstanceId::new(format!("i-{:016x}", s.next_id)).expect("ids valid")
    }
}

impl Default for InMemoryCompute { fn default() -> Self { Self::new() }}

#[async_trait]
impl CloudCompute for InMemoryCompute {
    async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId> {
        let mut s = self.state.lock().expect("mutex");
        let id = TemplateId(format!("lt-{}", s.templates.len()));
        s.templates.insert(spec.name.clone(), (id.clone(), spec.clone(), TemplateVersion(1)));
        Ok(id)
    }

    async fn template_update(&self, id: &TemplateId, spec: &TemplateSpec) -> Result<TemplateVersion> {
        let mut s = self.state.lock().expect("mutex");
        let entry = s.templates.values_mut().find(|(tid, _, _)| tid == id)
            .ok_or_else(|| Error::ConfigInvalid { reason: format!("template not found: {}", id.0) })?;
        entry.1 = spec.clone();
        entry.2 = TemplateVersion(entry.2.0 + 1);
        Ok(entry.2.clone())
    }

    async fn template_list(&self) -> Result<Vec<TemplateSummary>> {
        let s = self.state.lock().expect("mutex");
        Ok(s.templates.iter().map(|(name, (id, _, ver))| TemplateSummary {
            id: id.clone(), name: name.clone(), latest_version: ver.clone(),
        }).collect())
    }

    async fn template_delete(&self, id: &TemplateId) -> Result<()> {
        let mut s = self.state.lock().expect("mutex");
        s.templates.retain(|_, (tid, _, _)| tid != id);
        Ok(())
    }

    async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>> {
        let s = self.state.lock().expect("mutex");
        Ok(s.templates.get(name).map(|(id, _, ver)| TemplateSummary {
            id: id.clone(), name: name.clone(), latest_version: ver.clone(),
        }))
    }

    async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance> {
        let id = self.next_instance_id();
        let tags = vec![
            Tag::new(KLEYA_TAG_NAME,     req.instance_name.as_str())?,
            Tag::new(KLEYA_TAG_MANAGED,  "true")?,
            Tag::new(KLEYA_TAG_TEMPLATE, &req.template.0)?,
            Tag::new(KLEYA_TAG_KEY,      req.key_name.as_str())?,
        ];
        let inst = Instance {
            id: id.clone(),
            name: Some(req.instance_name.clone()),
            state: InstanceState::Pending,
            public_dns: Some(format!("{}.example", id.as_str())),
            public_ip:  Some("203.0.113.10".into()),
            tags,
        };
        self.state.lock().expect("mutex").instances.insert(id.clone(), inst.clone());
        Ok(inst)
    }

    async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>> {
        let s = self.state.lock().expect("mutex");
        let out = s.instances.values().filter(|i| {
            if filter.managed_only && !i.tags.iter().any(|t| t.key == KLEYA_TAG_MANAGED) {
                return false;
            }
            if let Some(n) = &filter.name {
                if i.name.as_ref().map(|x| x.as_str()) != Some(n.as_str()) { return false; }
            }
            if !filter.states.is_empty() && !filter.states.contains(&i.state) { return false; }
            true
        }).cloned().collect();
        Ok(out)
    }

    async fn instance_describe(&self, id: &InstanceId) -> Result<Instance> {
        self.state.lock().expect("mutex").instances.get(id).cloned()
            .ok_or_else(|| Error::InstanceNotFound { name: id.as_str().into(), region: "fake".into() })
    }

    async fn instance_terminate(&self, id: &InstanceId) -> Result<()> {
        let mut s = self.state.lock().expect("mutex");
        if let Some(i) = s.instances.get_mut(id) {
            i.state = InstanceState::Terminated;
        }
        Ok(())
    }

    async fn instance_wait_running(&self, id: &InstanceId, _deadline: Deadline) -> Result<Instance> {
        let mut s = self.state.lock().expect("mutex");
        let i = s.instances.get_mut(id)
            .ok_or_else(|| Error::InstanceNotFound { name: id.as_str().into(), region: "fake".into() })?;
        i.state = InstanceState::Running;
        Ok(i.clone())
    }

    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId> {
        let mut s = self.state.lock().expect("mutex");
        let id = s.sgs.entry(name.to_string())
            .or_insert(SecurityGroupId(format!("sg-{}", s.sgs.len() + 1))).clone();
        Ok(id)
    }

    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()> {
        let mut s = self.state.lock().expect("mutex");
        s.keypairs.entry(name.clone())
            .or_insert(format!("md5:{:x}", crc32fast::hash(public_key.0.as_bytes())));
        Ok(())
    }

    async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<String>> {
        Ok(self.state.lock().expect("mutex").keypairs.get(name).cloned())
    }

    async fn resolve_default_subnet(&self) -> Result<SubnetId> { Ok(self.default_subnet.clone()) }
    async fn resolve_ami_alias(&self, _alias: &str) -> Result<AmiId> { Ok(self.default_ami.clone()) }
}
```

Add `crc32fast` as a dev-aux for the fake fingerprint:
```toml
[dev-dependencies]
crc32fast = "1"
```
Actually move it under `[dependencies]` gated behind `test-support` to avoid leakage; or keep it as a plain dep — `crc32fast` is tiny and acceptable to ship.

Update `crates/kleya-core/Cargo.toml`:
```toml
crc32fast = "1"
```

- [ ] **Step 6: Verify**

```bash
cargo check -p kleya-core --features test-support
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 11: Commands — template create/update/list/delete

**Files:**
- Modify: `crates/kleya-core/src/commands/mod.rs`
- Create: `crates/kleya-core/src/commands/template.rs`
- Test: `crates/kleya-core/tests/commands_with_fakes.rs`

- [ ] **Step 1: Wire `commands/mod.rs`**

```rust
pub mod template;
// pub mod launch;       // Task 12
// pub mod list;         // Task 13
// pub mod connect;      // Task 14
// pub mod terminate;    // Task 15
```

- [ ] **Step 2: Write `commands/template.rs`**

```rust
use std::sync::Arc;

use crate::Result;
use crate::config::Config;
use crate::model::template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion};
use crate::ports::cloud_compute::CloudCompute;

pub struct TemplateService {
    pub compute: Arc<dyn CloudCompute>,
    pub config:  Arc<Config>,
}

impl TemplateService {
    pub async fn create(&self, spec: TemplateSpec) -> Result<TemplateId> {
        assert!(!spec.name.0.is_empty(), "template name empty");
        self.compute.template_create(&spec).await
    }

    pub async fn update(&self, id: &TemplateId, spec: TemplateSpec) -> Result<TemplateVersion> {
        assert!(!id.0.is_empty(), "template id empty");
        self.compute.template_update(id, &spec).await
    }

    pub async fn list(&self) -> Result<Vec<TemplateSummary>> { self.compute.template_list().await }

    pub async fn delete_by_name(&self, name: &TemplateName) -> Result<()> {
        let summary = self.compute.template_get_by_name(name).await?
            .ok_or_else(|| crate::error::Error::ConfigInvalid {
                reason: format!("template '{}' not found", name.0),
            })?;
        self.compute.template_delete(&summary.id).await
    }
}
```

- [ ] **Step 3: Integration test against fakes**

`crates/kleya-core/tests/commands_with_fakes.rs`:
```rust
use std::sync::Arc;

use kleya_core::commands::template::TemplateService;
use kleya_core::config::Config;
use kleya_core::model::{
    key::KeyName, market::{MarketKind, SpotType}, region::AmiId,
    template::{TemplateName, TemplateSpec},
};
use kleya_core::test_support::InMemoryCompute;

fn sample_spec(name: &str) -> TemplateSpec {
    TemplateSpec {
        name: TemplateName(name.into()),
        ami_id: Some(AmiId("ami-1".into())),
        ami_alias: None,
        instance_type: "m8g.xlarge".into(),
        key_name: KeyName::new("kleya-default").unwrap(),
        security_group_ids: vec![],
        subnet_id: None,
        market: MarketKind::Spot,
        spot_type: SpotType::OneTime,
        tags: vec![],
        user_data_base64: "H4sIAAAA".into(),
    }
}

#[tokio::test]
async fn create_then_list_then_delete() {
    let svc = TemplateService {
        compute: Arc::new(InMemoryCompute::new()),
        config:  Arc::new(Config::default()),
    };
    svc.create(sample_spec("devbox")).await.expect("create");
    svc.create(sample_spec("workbox")).await.expect("create");
    let listed = svc.list().await.expect("list");
    assert_eq!(listed.len(), 2);

    svc.delete_by_name(&TemplateName("devbox".into())).await.expect("delete");
    let listed = svc.list().await.expect("list");
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn delete_unknown_returns_error() {
    let svc = TemplateService {
        compute: Arc::new(InMemoryCompute::new()),
        config:  Arc::new(Config::default()),
    };
    let err = svc.delete_by_name(&TemplateName("ghost".into())).await.unwrap_err();
    assert!(matches!(err, kleya_core::Error::ConfigInvalid { .. }));
}
```

Add to `crates/kleya-core/Cargo.toml`:
```toml
[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt"] }
```

- [ ] **Step 4: Verify**

```bash
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 12: Command — `launch` (zero-config orchestration)

**Files:**
- Create: `crates/kleya-core/src/commands/launch.rs`
- Modify: `crates/kleya-core/src/commands/mod.rs`
- Modify: tests in `commands_with_fakes.rs`

- [ ] **Step 1: Wire and write `commands/launch.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::Result;
use crate::bootstrap::{encode::encode_user_data, render::{render, BootstrapVars}};
use crate::config::Config;
use crate::limits::{LAUNCH_POLL_INTERVAL_SECONDS, LAUNCH_WAIT_SECONDS_MAX};
use crate::model::{
    instance::{Instance, InstanceName},
    key::KeyName,
    launch::{Deadline, LaunchRequest},
    market::{MarketKind, SpotType},
    region::AmiId,
    tag::Tag,
    template::{TemplateName, TemplateSpec},
};
use crate::ports::{cloud_compute::CloudCompute, id_gen::IdGen, key_store::KeyStore};

pub struct LaunchService {
    pub compute:        Arc<dyn CloudCompute>,
    pub key_store:      Arc<dyn KeyStore>,
    pub id_gen:         Arc<dyn IdGen>,
    pub config:         Arc<Config>,
    pub bootstrap_tpl:  &'static str,
    pub ghostty_tinfo:  &'static str,
}

pub struct LaunchOpts {
    pub template_name: Option<String>,
    pub instance_name: Option<String>,
    pub dry_run: bool,
}

pub struct LaunchPlan {
    pub template: TemplateName,
    pub instance_name: InstanceName,
    pub key_name: KeyName,
    pub ami_id:   AmiId,
}

impl LaunchService {
    pub async fn run(&self, opts: LaunchOpts) -> Result<Option<Instance>> {
        let plan = self.build_plan(&opts).await?;
        if opts.dry_run {
            tracing::info!(template = %plan.template.0, instance = %plan.instance_name.as_str(),
                           key = %plan.key_name.as_str(), ami = %plan.ami_id.0,
                           "dry-run plan");
            return Ok(None);
        }
        self.ensure_template(&plan).await?;
        let inst = self.compute.instance_launch(&LaunchRequest {
            template:      plan.template.clone(),
            instance_name: plan.instance_name.clone(),
            instance_type_override: None,
            market_override:        None,
            spot_type_override:     None,
            extra_tags: vec![],
            key_name:   plan.key_name.clone(),
        }).await?;
        let deadline = Deadline {
            timeout: Duration::from_secs(u64::from(LAUNCH_WAIT_SECONDS_MAX)),
            poll_interval: Duration::from_secs(u64::from(LAUNCH_POLL_INTERVAL_SECONDS)),
        };
        let running = self.compute.instance_wait_running(&inst.id, deadline).await?;
        Ok(Some(running))
    }

    async fn build_plan(&self, opts: &LaunchOpts) -> Result<LaunchPlan> {
        let template_name = TemplateName(
            opts.template_name.clone().unwrap_or_else(|| "default".into()));
        let instance_name = match &opts.instance_name {
            Some(n) => InstanceName::new(n)?,
            None    => InstanceName::new(self.id_gen.name())?,
        };
        let key_name = KeyName::new(self.config.keys.default_key_name.clone())?;
        let ami_id = self.compute.resolve_ami_alias(&self.config.defaults.ami_alias).await?;
        Ok(LaunchPlan { template: template_name, instance_name, key_name, ami_id })
    }

    async fn ensure_template(&self, plan: &LaunchPlan) -> Result<()> {
        if self.compute.template_get_by_name(&plan.template).await?.is_some() {
            self.assert_key_synced(&plan.key_name).await?;
            return Ok(());
        }
        let subnet = self.compute.resolve_default_subnet().await?;
        let sg     = self.compute.ensure_default_security_group("kleya-default").await?;
        self.ensure_keypair(&plan.key_name).await?;
        let vars = BootstrapVars::default_with(self.ghostty_tinfo);
        let rendered = render(self.bootstrap_tpl, &vars)?;
        let user_data_b64 = encode_user_data(&rendered)?;
        let spec = TemplateSpec {
            name: plan.template.clone(),
            ami_id: Some(plan.ami_id.clone()),
            ami_alias: None,
            instance_type: self.config.defaults.instance_type.clone(),
            key_name: plan.key_name.clone(),
            security_group_ids: vec![sg],
            subnet_id: Some(subnet),
            market: match self.config.defaults.market.as_str() {
                "on-demand" => MarketKind::OnDemand, _ => MarketKind::Spot,
            },
            spot_type: match self.config.defaults.spot_type.as_str() {
                "persistent" => SpotType::Persistent, _ => SpotType::OneTime,
            },
            tags: vec![Tag::new("Project", "kleya")?],
            user_data_base64: user_data_b64,
        };
        self.compute.template_create(&spec).await?;
        Ok(())
    }

    async fn ensure_keypair(&self, name: &kleya_core::model::key::KeyName) -> Result<()> {
        match (self.key_store.exists(name), self.compute.keypair_fingerprint(name).await?) {
            (true, Some(cloud_fp)) => {
                let local_fp = self.key_store.fingerprint(name)?.0;
                if local_fp != cloud_fp {
                    return Err(crate::error::Error::KeyMismatch { name: name.to_string() });
                }
                Ok(())
            }
            (true, None) => {
                let public = self.key_store.read_public(name)?;
                self.compute.ensure_default_keypair(name, &public).await
            }
            (false, Some(_)) => Err(crate::error::Error::KeyOrphaned { name: name.to_string() }),
            (false, None) => {
                let pair = self.key_store.generate(name)?;
                self.compute.ensure_default_keypair(name, &pair.public).await
            }
        }
    }

    async fn assert_key_synced(&self, name: &kleya_core::model::key::KeyName) -> Result<()> {
        // Lighter-weight check on the existing-template path: verify the local
        // key is present. Full fingerprint check still runs via ensure_keypair
        // when a fresh template is created.
        if !self.key_store.exists(name) {
            return Err(crate::error::Error::KeyOrphaned { name: name.to_string() });
        }
        Ok(())
    }
}
```

Note the absolute `crate::error::Error` references inside `ensure_keypair`/`assert_key_synced` — these match the existing imports at the top of `commands/launch.rs`. Adjust if the `use` block already brings `Error` into scope.

In `commands/mod.rs` uncomment `pub mod launch;`.

- [ ] **Step 2: Test in `commands_with_fakes.rs`**

Append:
```rust
use kleya_core::commands::launch::{LaunchOpts, LaunchService};
use kleya_core::ports::id_gen::IdGen;
use kleya_core::test_support::{FakeIdGen, InMemoryKeyStore};

#[tokio::test]
async fn launch_zero_config_creates_default_template_and_instance() {
    let svc = LaunchService {
        compute:   Arc::new(InMemoryCompute::new()),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen:    Arc::new(FakeIdGen::new()),
        config:    Arc::new(Config::default()),
        bootstrap_tpl: "echo hi",
        ghostty_tinfo: "",
    };
    let res = svc.run(LaunchOpts { template_name: None, instance_name: None, dry_run: false })
        .await.expect("launch ok");
    let inst = res.expect("returned instance");
    assert!(matches!(inst.state, kleya_core::model::instance::InstanceState::Running));
}

#[tokio::test]
async fn launch_dry_run_returns_none_and_does_not_create_template() {
    let compute = Arc::new(InMemoryCompute::new());
    let svc = LaunchService {
        compute: compute.clone(),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen:  Arc::new(FakeIdGen::new()),
        config:  Arc::new(Config::default()),
        bootstrap_tpl: "echo hi",
        ghostty_tinfo: "",
    };
    let res = svc.run(LaunchOpts { template_name: None, instance_name: None, dry_run: true })
        .await.expect("dry-run ok");
    assert!(res.is_none());
    assert!(compute.template_list().await.unwrap().is_empty());
}
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 13: Commands — `list` and `terminate`

**Files:**
- Create: `crates/kleya-core/src/commands/{list.rs, terminate.rs}`
- Modify: `crates/kleya-core/src/commands/mod.rs`
- Test: append to `commands_with_fakes.rs`

- [ ] **Step 1: `list.rs`**

```rust
use std::sync::Arc;
use crate::Result;
use crate::model::instance::{Instance, InstanceFilter};
use crate::ports::cloud_compute::CloudCompute;

pub struct ListService { pub compute: Arc<dyn CloudCompute> }

impl ListService {
    pub async fn list_managed(&self) -> Result<Vec<Instance>> {
        let filter = InstanceFilter { name: None, managed_only: true, states: vec![] };
        self.compute.instance_list(&filter).await
    }
}
```

- [ ] **Step 2: `terminate.rs`**

```rust
use std::sync::Arc;
use crate::Result;
use crate::error::Error;
use crate::model::instance::{InstanceFilter, InstanceId};
use crate::ports::cloud_compute::CloudCompute;

pub struct TerminateService {
    pub compute: Arc<dyn CloudCompute>,
    pub region:  String,
}

impl TerminateService {
    pub async fn terminate_by_handle(&self, handle: &str) -> Result<InstanceId> {
        let id = if handle.starts_with("i-") {
            InstanceId::new(handle)?
        } else {
            let candidates = self.compute.instance_list(&InstanceFilter {
                name: Some(handle.into()), managed_only: true, states: vec![],
            }).await?;
            match candidates.len() {
                0 => return Err(Error::InstanceNotFound {
                    name: handle.into(), region: self.region.clone(),
                }),
                1 => candidates[0].id.clone(),
                n => return Err(Error::AmbiguousHandle {
                    name: handle.into(),
                    count: n,
                    candidates: candidates.iter().map(|i| i.id.as_str().to_string()).collect(),
                }),
            }
        };
        self.compute.instance_terminate(&id).await?;
        Ok(id)
    }
}
```

- [ ] **Step 3: Mod wiring**

In `commands/mod.rs`:
```rust
pub mod list;
pub mod terminate;
pub mod template;
pub mod launch;
pub mod connect;     // Task 14
```

- [ ] **Step 4: Tests**

Append:
```rust
use kleya_core::commands::{list::ListService, terminate::TerminateService};

#[tokio::test]
async fn terminate_by_name_succeeds_when_unique() {
    let compute = Arc::new(InMemoryCompute::new());
    let svc = LaunchService {
        compute: compute.clone(),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen:  Arc::new(FakeIdGen::new()),
        config:  Arc::new(Config::default()),
        bootstrap_tpl: "echo hi",
        ghostty_tinfo: "",
    };
    let inst = svc.run(LaunchOpts {
        template_name: None,
        instance_name: Some("solo".into()),
        dry_run: false,
    }).await.expect("launch").expect("inst");

    let term = TerminateService { compute: compute.clone(), region: "eu-west-1".into() };
    let id = term.terminate_by_handle("solo").await.expect("terminate");
    assert_eq!(id, inst.id);
}

#[tokio::test]
async fn terminate_unknown_returns_not_found() {
    let compute = Arc::new(InMemoryCompute::new());
    let term = TerminateService { compute, region: "eu-west-1".into() };
    let err = term.terminate_by_handle("ghost").await.unwrap_err();
    assert!(matches!(err, kleya_core::Error::InstanceNotFound { .. }));
}

#[tokio::test]
async fn list_returns_only_managed() {
    let compute = Arc::new(InMemoryCompute::new());
    let svc = LaunchService {
        compute: compute.clone(),
        key_store: Arc::new(InMemoryKeyStore::new()),
        id_gen:  Arc::new(FakeIdGen::new()),
        config:  Arc::new(Config::default()),
        bootstrap_tpl: "echo hi",
        ghostty_tinfo: "",
    };
    svc.run(LaunchOpts { template_name: None, instance_name: Some("a".into()), dry_run: false })
        .await.unwrap();
    svc.run(LaunchOpts { template_name: None, instance_name: Some("b".into()), dry_run: false })
        .await.unwrap();
    let list = ListService { compute }.list_managed().await.expect("list");
    assert_eq!(list.len(), 2);
}
```

- [ ] **Step 5: Verify**

```bash
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 14: Command — `connect` (handle resolve, key lookup, ssh argv, probe)

**Files:**
- Create: `crates/kleya-core/src/commands/connect.rs`

- [ ] **Step 1: `connect.rs`**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::Result;
use crate::config::Config;
use crate::error::Error;
use crate::limits::{
    SSH_PROBE_INTERVAL_SECONDS, SSH_PROBE_PORT, SSH_PROBE_TCP_TIMEOUT_MS, SSH_PROBE_TIMEOUT_SECONDS,
};
use crate::model::instance::{Instance, InstanceFilter, InstanceId};
use crate::model::key::KeyName;
use crate::model::tag::{KLEYA_TAG_KEY, KLEYA_TAG_MANAGED};
use crate::ports::cloud_compute::CloudCompute;
use crate::ports::key_store::KeyStore;

pub struct ConnectService {
    pub compute:   Arc<dyn CloudCompute>,
    pub key_store: Arc<dyn KeyStore>,
    pub config:    Arc<Config>,
    pub region:    String,
}

pub struct ConnectPlan {
    pub argv: Vec<String>,
    pub instance_id: InstanceId,
    pub endpoint: String,
    pub key_path: PathBuf,
}

pub struct ConnectOpts {
    pub handle: String,
    pub explicit_instance_id: Option<String>,
    pub no_tmux: bool,
    pub tmux_session: Option<String>,
}

impl ConnectService {
    pub async fn plan(&self, opts: &ConnectOpts) -> Result<ConnectPlan> {
        let inst = self.resolve(opts).await?;
        let key_name = self.resolve_key(&inst)?;
        let key_path = self.key_store.private_path(&key_name)?;
        let endpoint = inst.public_dns.clone().ok_or_else(|| Error::ConfigInvalid {
            reason: format!("instance {} has no public DNS", inst.id.as_str()),
        })?;
        let argv = self.build_argv(&endpoint, &key_path, opts);
        Ok(ConnectPlan { argv, instance_id: inst.id, endpoint, key_path })
    }

    async fn resolve(&self, opts: &ConnectOpts) -> Result<Instance> {
        if let Some(id) = &opts.explicit_instance_id {
            return self.compute.instance_describe(&InstanceId::new(id)?).await;
        }
        if opts.handle.starts_with("i-") {
            return self.compute.instance_describe(&InstanceId::new(&opts.handle)?).await;
        }
        let candidates = self.compute.instance_list(&InstanceFilter {
            name: Some(opts.handle.clone()), managed_only: true, states: vec![],
        }).await?;
        match candidates.len() {
            0 => Err(Error::InstanceNotFound {
                name: opts.handle.clone(), region: self.region.clone(),
            }),
            1 => Ok(candidates.into_iter().next().expect("len==1")),
            n => Err(Error::AmbiguousHandle {
                name: opts.handle.clone(), count: n,
                candidates: candidates.iter().map(|i| i.id.as_str().to_string()).collect(),
            }),
        }
    }

    fn resolve_key(&self, inst: &Instance) -> Result<KeyName> {
        let tagged = inst.tags.iter().find(|t| t.key == KLEYA_TAG_KEY).map(|t| t.value.clone());
        if let Some(n) = tagged { return KeyName::new(n); }
        let managed = inst.tags.iter().any(|t| t.key == KLEYA_TAG_MANAGED && t.value == "true");
        if !managed {
            return Err(Error::ConfigInvalid {
                reason: format!("instance {} not managed by kleya; pass --instance-id and configure key",
                                inst.id.as_str()),
            });
        }
        KeyName::new(self.config.keys.default_key_name.clone())
    }

    fn build_argv(&self, endpoint: &str, key_path: &PathBuf, opts: &ConnectOpts) -> Vec<String> {
        let mut argv: Vec<String> = vec!["ssh".into()];
        argv.push("-i".into());
        argv.push(key_path.to_string_lossy().into_owned());
        argv.push("-o".into()); argv.push("StrictHostKeyChecking=accept-new".into());
        argv.push("-o".into()); argv.push("ServerAliveInterval=30".into());
        argv.push("-o".into()); argv.push("ConnectTimeout=10".into());
        for a in &self.config.ssh.extra_args { argv.push(a.clone()); }
        argv.push("-t".into());
        argv.push(format!("{}@{endpoint}", self.config.ssh.user));
        if !opts.no_tmux && self.config.ssh.tmux {
            let session = opts.tmux_session.clone().unwrap_or_else(|| self.config.ssh.tmux_session.clone());
            argv.push("tmux".into());
            argv.push("new-session".into());
            argv.push("-A".into());
            argv.push("-s".into());
            argv.push(session);
        }
        argv
    }
}

#[must_use]
pub fn probe_timing() -> (Duration, Duration, u16) {
    (
        Duration::from_secs(u64::from(SSH_PROBE_TIMEOUT_SECONDS)),
        Duration::from_secs(u64::from(SSH_PROBE_INTERVAL_SECONDS)),
        SSH_PROBE_PORT,
    )
}

pub const TCP_TIMEOUT_MS: u32 = SSH_PROBE_TCP_TIMEOUT_MS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::InMemoryCompute;
    use crate::test_support::InMemoryKeyStore;
    use crate::commands::launch::{LaunchOpts, LaunchService};
    use crate::test_support::FakeIdGen;

    #[tokio::test]
    async fn build_argv_includes_tmux_by_default() {
        let compute   = Arc::new(InMemoryCompute::new());
        let key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let cfg = Arc::new(Config::default());
        let svc = ConnectService {
            compute:   compute.clone(),
            key_store: key_store.clone(),
            config:    cfg.clone(),
            region:    "eu-west-1".into(),
        };
        // seed an instance via launch
        let l = LaunchService {
            compute, key_store, id_gen: Arc::new(FakeIdGen::new()),
            config: cfg, bootstrap_tpl: "echo hi", ghostty_tinfo: "",
        };
        l.run(LaunchOpts { template_name: None, instance_name: Some("box".into()), dry_run: false })
            .await.unwrap();

        let plan = svc.plan(&ConnectOpts {
            handle: "box".into(), explicit_instance_id: None,
            no_tmux: false, tmux_session: None,
        }).await.expect("plan ok");
        assert!(plan.argv.iter().any(|a| a == "tmux"));
        assert!(plan.argv.iter().any(|a| a == "kleya"));
    }
}
```

- [ ] **Step 2: Verify**

```bash
cargo nextest run -p kleya-core
```
Expected: PASS.

---

## Task 15: AWS adapter — crate skeleton + client builder

**Files:**
- Modify: `crates/kleya-aws/Cargo.toml`
- Create: `crates/kleya-aws/src/{lib.rs,error.rs,client.rs,mapping.rs,ec2.rs}`

- [ ] **Step 1: Add deps**

```toml
[dependencies]
kleya-core = { path = "../kleya-core" }
aws-config = "1"
aws-sdk-ec2 = "1"
aws-sdk-ssm = "1"
aws-credential-types = "1"
aws-smithy-types = "1"
aws-smithy-runtime-api = "1"
async-trait = { workspace = true }
thiserror  = { workspace = true }
tokio      = { workspace = true, features = ["macros", "rt", "time"] }
tracing    = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "time"] }
serial_test = "3"
```

- [ ] **Step 2: `error.rs`**

```rust
use kleya_core::error::{BoxError, Error as CoreError};

#[derive(Debug, thiserror::Error)]
pub enum AwsError {
    #[error("ec2 sdk: {0}")]
    Sdk(#[from] BoxError),
    #[error("missing field in response: {0}")]
    MissingField(&'static str),
    #[error("ssm parameter not found: {0}")]
    SsmMissing(String),
}

impl From<AwsError> for CoreError {
    fn from(e: AwsError) -> Self { CoreError::adapter("aws-ec2", e) }
}
```

- [ ] **Step 3: `client.rs`**

```rust
use aws_config::BehaviorVersion;
use aws_sdk_ec2::config::Region;
use aws_sdk_ec2::Client as Ec2Client;

#[must_use]
pub async fn build_ec2_client(region: &str, endpoint_url: Option<&str>) -> Ec2Client {
    let mut loader = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(region.to_string()));
    if let Some(url) = endpoint_url { loader = loader.endpoint_url(url); }
    let cfg = loader.load().await;
    Ec2Client::new(&cfg)
}
```

- [ ] **Step 4: `mapping.rs` (stubs we'll fill as ec2.rs needs them)**

```rust
use aws_sdk_ec2::types as e;
use kleya_core::model::{
    instance::{Instance, InstanceId, InstanceName, InstanceState},
    tag::Tag,
};

pub fn map_instance(i: &e::Instance) -> Option<Instance> {
    let id = InstanceId::new(i.instance_id()?).ok()?;
    let state = match i.state().and_then(|s| s.name()) {
        Some(e::InstanceStateName::Pending)       => InstanceState::Pending,
        Some(e::InstanceStateName::Running)       => InstanceState::Running,
        Some(e::InstanceStateName::ShuttingDown)  => InstanceState::ShuttingDown,
        Some(e::InstanceStateName::Stopped)       => InstanceState::Stopped,
        Some(e::InstanceStateName::Stopping)      => InstanceState::Stopping,
        Some(e::InstanceStateName::Terminated)    => InstanceState::Terminated,
        Some(other) => InstanceState::Other(other.as_str().into()),
        None        => InstanceState::Other("unknown".into()),
    };
    let tags: Vec<Tag> = i.tags().iter().filter_map(|t|
        Tag::new(t.key()?, t.value()?).ok()).collect();
    let name = tags.iter().find(|t| t.key == "Name")
        .and_then(|t| InstanceName::new(&t.value).ok());
    Some(Instance {
        id, name, state,
        public_dns: i.public_dns_name().map(str::to_string).filter(|s| !s.is_empty()),
        public_ip:  i.public_ip_address().map(str::to_string),
        tags,
    })
}
```

- [ ] **Step 5: `lib.rs`**

```rust
//! kleya-aws: EC2 adapter for `kleya_core::ports::CloudCompute`.

pub mod client;
pub mod ec2;
pub mod error;
pub mod mapping;
```

- [ ] **Step 6: `ec2.rs` — empty CloudCompute impl to keep workspace green**

```rust
use async_trait::async_trait;
use std::sync::Arc;

use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;

use kleya_core::Result;
use kleya_core::model::{
    instance::{Instance, InstanceFilter, InstanceId},
    key::{KeyName, PublicKey},
    launch::{Deadline, LaunchRequest},
    region::{AmiId, SecurityGroupId, SubnetId},
    template::{TemplateId, TemplateName, TemplateSpec, TemplateSummary, TemplateVersion},
};
use kleya_core::ports::cloud_compute::CloudCompute;

pub struct AwsEc2 {
    pub ec2: Arc<Ec2Client>,
    pub ssm: Arc<SsmClient>,
    pub region: String,
}

#[async_trait]
impl CloudCompute for AwsEc2 {
    async fn template_create(&self, _spec: &TemplateSpec) -> Result<TemplateId> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn template_update(&self, _id: &TemplateId, _spec: &TemplateSpec) -> Result<TemplateVersion> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn template_list(&self) -> Result<Vec<TemplateSummary>> { Ok(vec![]) }
    async fn template_delete(&self, _id: &TemplateId) -> Result<()> { Ok(()) }
    async fn template_get_by_name(&self, _name: &TemplateName) -> Result<Option<TemplateSummary>> { Ok(None) }
    async fn instance_launch(&self, _req: &LaunchRequest) -> Result<Instance> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn instance_list(&self, _filter: &InstanceFilter) -> Result<Vec<Instance>> { Ok(vec![]) }
    async fn instance_describe(&self, _id: &InstanceId) -> Result<Instance> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn instance_terminate(&self, _id: &InstanceId) -> Result<()> { Ok(()) }
    async fn instance_wait_running(&self, _id: &InstanceId, _d: Deadline) -> Result<Instance> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn ensure_default_security_group(&self, _name: &str) -> Result<SecurityGroupId> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn ensure_default_keypair(&self, _name: &KeyName, _public_key: &PublicKey) -> Result<()> { Ok(()) }
    async fn keypair_fingerprint(&self, _name: &KeyName) -> Result<Option<String>> { Ok(None) }
    async fn resolve_default_subnet(&self) -> Result<SubnetId> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
    async fn resolve_ami_alias(&self, _alias: &str) -> Result<AmiId> {
        Err(kleya_core::Error::ConfigInvalid { reason: "not implemented (Task 16)".into() })
    }
}
```

- [ ] **Step 7: Verify**

```bash
cargo check -p kleya-aws
cargo clippy -p kleya-aws --all-targets -- -D warnings
```
Expected: PASS.

---

## Task 16: AWS adapter — template + instance lifecycle implementation

**Files:**
- Modify: `crates/kleya-aws/src/ec2.rs`
- Modify: `crates/kleya-aws/src/mapping.rs`

Implement each `CloudCompute` method using `aws-sdk-ec2`. Sketch (each method ≤ 70 lines).

> **SDK version-tolerance note:** `aws-sdk-ec2 = "1"` follows a fast release cadence. If a method/type below has been renamed (e.g., `tags()` → `tags`, builder fields, or `InstanceType::from(&str)`), adapt to the current API for whichever 1.x `cargo` resolves — **do not pin to an older minor**. The shapes (`run_instances`, `describe_instances`, `create_launch_template`, `import_key_pair`, etc.) are stable; only call ergonomics drift.

- [ ] **Step 1: `template_create`**

```rust
async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId> {
    use aws_sdk_ec2::types as e;
    let tags: Vec<e::Tag> = spec.tags.iter().map(|t|
        e::Tag::builder().key(&t.key).value(&t.value).build()).collect();
    let market = e::InstanceMarketOptionsRequest::builder()
        .market_type(match spec.market {
            kleya_core::model::market::MarketKind::Spot      => e::MarketType::Spot,
            kleya_core::model::market::MarketKind::OnDemand  => return Err(self.unsupported("on-demand")),
        })
        .spot_options(
            e::SpotMarketOptions::builder()
                .spot_instance_type(match spec.spot_type {
                    kleya_core::model::market::SpotType::OneTime    => e::SpotInstanceType::OneTime,
                    kleya_core::model::market::SpotType::Persistent => e::SpotInstanceType::Persistent,
                }).build()
        ).build();
    let mut req = self.ec2.create_launch_template()
        .launch_template_name(&spec.name.0)
        .launch_template_data(
            e::RequestLaunchTemplateData::builder()
                .image_id(spec.ami_id.as_ref().map(|a| a.0.clone()).unwrap_or_default())
                .instance_type(e::InstanceType::from(spec.instance_type.as_str()))
                .key_name(spec.key_name.as_str())
                .user_data(&spec.user_data_base64)
                .instance_market_options(market)
                .set_security_group_ids(Some(spec.security_group_ids.iter().map(|s| s.0.clone()).collect()))
                .set_tag_specifications(Some(vec![
                    e::LaunchTemplateTagSpecificationRequest::builder()
                        .resource_type(e::ResourceType::Instance)
                        .set_tags(Some(tags)).build()
                ]))
                .build());
    if let Some(subnet) = &spec.subnet_id {
        req = req; // subnet is per-network-interface; left at default for v1
        let _ = subnet;
    }
    let out = req.send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let lt = out.launch_template().ok_or(crate::error::AwsError::MissingField("launch_template"))?;
    Ok(TemplateId(lt.launch_template_id().unwrap_or_default().to_string()))
}

fn unsupported(&self, what: &str) -> kleya_core::Error {
    kleya_core::Error::ConfigInvalid { reason: format!("{what} not supported in v1") }
}
```

- [ ] **Step 2: `template_list`, `template_get_by_name`, `template_delete`, `template_update`**

```rust
async fn template_list(&self) -> Result<Vec<TemplateSummary>> {
    let out = self.ec2.describe_launch_templates().send().await
        .map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    Ok(out.launch_templates().iter().filter_map(|lt| Some(TemplateSummary {
        id:   TemplateId(lt.launch_template_id()?.to_string()),
        name: TemplateName(lt.launch_template_name()?.to_string()),
        latest_version: TemplateVersion(u64::try_from(lt.latest_version_number().unwrap_or(0)).unwrap_or(0)),
    })).collect())
}

async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>> {
    let out = self.ec2.describe_launch_templates()
        .launch_template_names(&name.0).send().await;
    let Ok(out) = out else { return Ok(None); };
    Ok(out.launch_templates().first().and_then(|lt| Some(TemplateSummary {
        id:   TemplateId(lt.launch_template_id()?.to_string()),
        name: name.clone(),
        latest_version: TemplateVersion(u64::try_from(lt.latest_version_number().unwrap_or(0)).unwrap_or(0)),
    })))
}

async fn template_delete(&self, id: &TemplateId) -> Result<()> {
    self.ec2.delete_launch_template().launch_template_id(&id.0).send().await
        .map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    Ok(())
}

async fn template_update(&self, id: &TemplateId, spec: &TemplateSpec) -> Result<TemplateVersion> {
    // Implementation: create_launch_template_version with new data, then modify default.
    use aws_sdk_ec2::types as e;
    let out = self.ec2.create_launch_template_version()
        .launch_template_id(&id.0)
        .launch_template_data(e::RequestLaunchTemplateData::builder()
            .instance_type(e::InstanceType::from(spec.instance_type.as_str()))
            .key_name(spec.key_name.as_str())
            .user_data(&spec.user_data_base64)
            .build())
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let ver = out.launch_template_version()
        .and_then(|v| v.version_number())
        .ok_or(crate::error::AwsError::MissingField("version_number"))?;
    self.ec2.modify_launch_template()
        .launch_template_id(&id.0)
        .default_version(ver.to_string())
        .send().await
        .map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    Ok(TemplateVersion(u64::try_from(ver).unwrap_or(0)))
}
```

- [ ] **Step 3: `instance_launch`, `instance_list`, `instance_describe`, `instance_terminate`, `instance_wait_running`**

```rust
async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance> {
    use aws_sdk_ec2::types as e;
    let tags = vec![
        e::Tag::builder().key("Name").value(req.instance_name.as_str()).build(),
        e::Tag::builder().key("kleya:managed").value("true").build(),
        e::Tag::builder().key("kleya:template").value(&req.template.0).build(),
        e::Tag::builder().key("kleya:key").value(req.key_name.as_str()).build(),
    ];
    let mut run = self.ec2.run_instances()
        .launch_template(e::LaunchTemplateSpecification::builder()
            .launch_template_name(&req.template.0).build())
        .min_count(1).max_count(1)
        .tag_specifications(e::TagSpecification::builder()
            .resource_type(e::ResourceType::Instance)
            .set_tags(Some(tags)).build());
    if let Some(t) = &req.instance_type_override {
        run = run.instance_type(e::InstanceType::from(t.as_str()));
    }
    let out = run.send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let inst = out.instances().first().ok_or(crate::error::AwsError::MissingField("instances[0]"))?;
    crate::mapping::map_instance(inst)
        .ok_or_else(|| crate::error::AwsError::MissingField("instance fields").into())
}

async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>> {
    use aws_sdk_ec2::types as e;
    let mut req = self.ec2.describe_instances();
    if filter.managed_only {
        req = req.filters(e::Filter::builder().name("tag:kleya:managed").values("true").build());
    }
    if let Some(n) = &filter.name {
        req = req.filters(e::Filter::builder().name("tag:Name").values(n).build());
    }
    let out = req.send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let mut acc = vec![];
    for r in out.reservations() {
        for i in r.instances() {
            if let Some(inst) = crate::mapping::map_instance(i) { acc.push(inst); }
        }
    }
    Ok(acc)
}

async fn instance_describe(&self, id: &InstanceId) -> Result<Instance> {
    let out = self.ec2.describe_instances().instance_ids(id.as_str())
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let inst = out.reservations().first().and_then(|r| r.instances().first())
        .ok_or_else(|| kleya_core::Error::InstanceNotFound {
            name: id.as_str().into(), region: self.region.clone(),
        })?;
    crate::mapping::map_instance(inst)
        .ok_or_else(|| crate::error::AwsError::MissingField("instance fields").into())
}

async fn instance_terminate(&self, id: &InstanceId) -> Result<()> {
    self.ec2.terminate_instances().instance_ids(id.as_str()).send().await
        .map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    Ok(())
}

async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance> {
    let start = std::time::Instant::now();
    loop {
        let inst = self.instance_describe(id).await?;
        if matches!(inst.state, kleya_core::model::instance::InstanceState::Running) {
            return Ok(inst);
        }
        if start.elapsed() >= deadline.timeout {
            return Err(kleya_core::Error::LaunchWaitTimeout {
                instance_id: id.as_str().into(),
                elapsed_seconds: u32::try_from(start.elapsed().as_secs()).unwrap_or(u32::MAX),
            });
        }
        tokio::time::sleep(deadline.poll_interval).await;
    }
}
```

- [ ] **Step 4: `ensure_default_*`, `resolve_*`, `keypair_fingerprint`**

```rust
async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId> {
    use aws_sdk_ec2::types as e;
    let q = self.ec2.describe_security_groups().group_names(name).send().await;
    if let Ok(out) = q {
        if let Some(g) = out.security_groups().first() {
            if let Some(id) = g.group_id() {
                return Ok(SecurityGroupId(id.to_string()));
            }
        }
    }
    let created = self.ec2.create_security_group()
        .group_name(name)
        .description("kleya managed default SG")
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let id = created.group_id().ok_or(crate::error::AwsError::MissingField("group_id"))?.to_string();
    self.ec2.authorize_security_group_ingress()
        .group_id(&id)
        .ip_permissions(e::IpPermission::builder()
            .ip_protocol("tcp").from_port(22).to_port(22)
            .ip_ranges(e::IpRange::builder().cidr_ip("0.0.0.0/0").build())
            .build())
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    Ok(SecurityGroupId(id))
}

async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()> {
    if let Ok(out) = self.ec2.describe_key_pairs().key_names(name.as_str()).send().await {
        if !out.key_pairs().is_empty() { return Ok(()); }
    }
    self.ec2.import_key_pair()
        .key_name(name.as_str())
        .public_key_material(aws_sdk_ec2::primitives::Blob::new(public_key.0.as_bytes()))
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    Ok(())
}

async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<String>> {
    let out = self.ec2.describe_key_pairs().key_names(name.as_str()).send().await;
    let Ok(out) = out else { return Ok(None); };
    Ok(out.key_pairs().first().and_then(|k| k.key_fingerprint()).map(str::to_string))
}

async fn resolve_default_subnet(&self) -> Result<SubnetId> {
    use aws_sdk_ec2::types as e;
    let vpcs = self.ec2.describe_vpcs()
        .filters(e::Filter::builder().name("isDefault").values("true").build())
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let vpc_id = vpcs.vpcs().first().and_then(|v| v.vpc_id())
        .ok_or_else(|| kleya_core::Error::ConfigInvalid {
            reason: format!("no default VPC in region {}", self.region),
        })?;
    let subs = self.ec2.describe_subnets()
        .filters(e::Filter::builder().name("vpc-id").values(vpc_id).build())
        .send().await.map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let id = subs.subnets().first().and_then(|s| s.subnet_id())
        .ok_or_else(|| kleya_core::Error::ConfigInvalid {
            reason: format!("no subnet in default VPC of region {}", self.region),
        })?;
    Ok(SubnetId(id.to_string()))
}

async fn resolve_ami_alias(&self, alias: &str) -> Result<AmiId> {
    let param = match alias {
        "amazon-linux-2023-arm64" =>
            "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64",
        "amazon-linux-2023-x86_64" =>
            "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64",
        other => return Err(kleya_core::Error::ConfigInvalid {
            reason: format!("unknown ami_alias '{other}'"),
        }),
    };
    let out = self.ssm.get_parameter().name(param).send().await
        .map_err(|e| crate::error::AwsError::Sdk(Box::new(e)))?;
    let val = out.parameter().and_then(|p| p.value())
        .ok_or_else(|| crate::error::AwsError::SsmMissing(param.into()))?;
    Ok(AmiId(val.to_string()))
}
```

- [ ] **Step 5: Verify compile**

```bash
cargo check -p kleya-aws
cargo clippy -p kleya-aws --all-targets -- -D warnings
```
Expected: PASS.

---

## Task 17: Floci-backed integration tests for `kleya-aws`

**Files:**
- Create: `crates/kleya-aws/tests/floci/{mod.rs, template_lifecycle.rs, instance_lifecycle.rs}`

- [ ] **Step 1: `floci/mod.rs` — harness**

```rust
use std::process::Command;
use std::sync::OnceLock;

pub const FLOCI_ENDPOINT_ENV: &str = "KLEYA_TEST_FLOCI_ENDPOINT";
pub const FLOCI_ENABLE_ENV:   &str = "KLEYA_TEST_FLOCI";
// Pin obtained via:
//   docker pull floci/floci:latest
//   docker inspect --format='{{index .RepoDigests 0}}' floci/floci:latest
// Replace the digest before merging; do not leave REPLACE_WITH_PIN.
pub const FLOCI_IMAGE:        &str = "floci/floci@sha256:REPLACE_WITH_PIN";
pub const FLOCI_PORT:         u16  = 4566;

static STARTED: OnceLock<()> = OnceLock::new();

pub fn ensure_floci() -> Option<String> {
    if std::env::var(FLOCI_ENABLE_ENV).is_err() { return None; }
    STARTED.get_or_init(|| {
        let _ = Command::new("docker").args(["rm","-f","kleya-floci"]).status();
        let status = Command::new("docker").args([
            "run","-d","--rm","--name","kleya-floci",
            "-p", &format!("{FLOCI_PORT}:{FLOCI_PORT}"),
            "-v","/var/run/docker.sock:/var/run/docker.sock",
            FLOCI_IMAGE,
        ]).status().expect("docker available");
        assert!(status.success(), "floci start failed");
        std::thread::sleep(std::time::Duration::from_millis(2000));
    });
    Some(std::env::var(FLOCI_ENDPOINT_ENV)
        .unwrap_or_else(|_| format!("http://localhost:{FLOCI_PORT}")))
}

pub async fn ec2(endpoint: &str) -> aws_sdk_ec2::Client {
    kleya_aws::client::build_ec2_client("eu-west-1", Some(endpoint)).await
}
```

Replace `REPLACE_WITH_PIN` once you run `docker pull floci/floci:latest && docker inspect ...` and capture the sha256.

- [ ] **Step 2: `template_lifecycle.rs`**

```rust
mod harness { include!("mod.rs"); }
use harness::*;
use std::sync::Arc;

#[tokio::test]
#[ignore]
async fn create_list_delete_template() {
    let Some(endpoint) = ensure_floci() else { return; };
    let ec2 = Arc::new(ec2(&endpoint).await);
    let ssm = Arc::new({
        use aws_config::BehaviorVersion;
        let cfg = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_sdk_ec2::config::Region::new("eu-west-1"))
            .load().await;
        aws_sdk_ssm::Client::new(&cfg)
    });
    let adapter = kleya_aws::ec2::AwsEc2 { ec2, ssm, region: "eu-west-1".into() };

    use kleya_core::model::*;
    let spec = kleya_core::model::template::TemplateSpec {
        name: template::TemplateName("floci-t1".into()),
        ami_id: Some(region::AmiId("ami-00000000000000001".into())),
        ami_alias: None,
        instance_type: "t3.micro".into(),
        key_name: key::KeyName::new("kleya-default").unwrap(),
        security_group_ids: vec![],
        subnet_id: None,
        market: market::MarketKind::Spot,
        spot_type: market::SpotType::OneTime,
        tags: vec![],
        user_data_base64: "H4sIAAAA".into(),
    };
    let id = <kleya_aws::ec2::AwsEc2 as kleya_core::ports::cloud_compute::CloudCompute>::
        template_create(&adapter, &spec).await.expect("create");
    let listed = <kleya_aws::ec2::AwsEc2 as kleya_core::ports::cloud_compute::CloudCompute>::
        template_list(&adapter).await.expect("list");
    assert!(listed.iter().any(|t| t.id == id));
    <kleya_aws::ec2::AwsEc2 as kleya_core::ports::cloud_compute::CloudCompute>::
        template_delete(&adapter, &id).await.expect("delete");
}
```

(Replicate analogous tests in `instance_lifecycle.rs` for `instance_launch` → `instance_list` → `instance_terminate`. Each test gated `#[ignore]`.)

- [ ] **Step 3: Verify gating (no Floci run)**

```bash
cargo nextest run -p kleya-aws
```
Expected: PASS, all `#[ignore]`'d tests skipped.

- [ ] **Step 4: Optional local Floci run (manual, not part of CI default)**

```bash
KLEYA_TEST_FLOCI=1 cargo nextest run -p kleya-aws --run-ignored
```
Expected: PASS when Floci is reachable.

---

## Task 18: CLI — clap args, dispatch, exit codes

**Files:**
- Modify: `crates/kleya-cli/Cargo.toml`
- Modify: `crates/kleya-cli/src/{main.rs,clap_args.rs,dispatch.rs,exit_code.rs,logging.rs}`

- [ ] **Step 1: Deps**

```toml
[dependencies]
kleya-core = { path = "../kleya-core", features = ["test-support"] }
kleya-aws  = { path = "../kleya-aws" }
kleya-bootstrap-assets = { path = "../kleya-bootstrap-assets" }
clap     = { workspace = true }
tokio    = { workspace = true, features = ["macros", "rt", "time", "net"] }
tracing  = { workspace = true }
tracing-subscriber = { workspace = true }
serde    = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
toml     = { workspace = true }
jsonc-parser = { workspace = true }
anyhow   = { workspace = true }
nix      = { workspace = true }
ssh-key  = { workspace = true }
md-5     = "0.10"
hex      = "0.4"
```

- [ ] **Step 2: `exit_code.rs`**

```rust
use kleya_core::Error;
pub fn code_for(err: &Error) -> i32 {
    match err {
        Error::ConfigInvalid { .. }       => 2,
        Error::InstanceNotFound { .. }    => 3,
        Error::AmbiguousHandle { .. }     => 4,
        Error::SshNotReady { .. }         => 5,
        Error::LaunchWaitTimeout { .. }   => 6,
        Error::KeyMismatch { .. }
        | Error::KeyOrphaned { .. }       => 7,
        Error::Adapter { .. }             => 70,
        Error::Io(_)                      => 74,
        Error::UserDataTooLarge { .. }
        | Error::TemplateRender(_)        => 1,
    }
}
```

- [ ] **Step 3: `logging.rs`**

```rust
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init(verbosity: u8, json: bool) {
    let level = match verbosity { 0 => "info", 1 => "debug", _ => "trace" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("kleya={level},warn")));
    if json {
        tracing_subscriber::registry()
            .with(fmt::layer().json().with_target(false))
            .with(filter).init();
    } else {
        tracing_subscriber::registry()
            .with(fmt::layer().with_target(false))
            .with(filter).init();
    }
}
```

- [ ] **Step 4: `clap_args.rs`**

```rust
use clap::{Parser, Subcommand, Args, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "kleya", version, about = "Bootstrap AWS spot dev boxes")]
pub struct Cli {
    #[command(subcommand)] pub command: Cmd,
    #[arg(long, global = true)] pub config: Option<String>,
    #[arg(long, global = true)] pub profile: Option<String>,
    #[arg(long, global = true)] pub region: Option<String>,
    #[arg(short = 'v', action = clap::ArgAction::Count, global = true)] pub verbose: u8,
    #[arg(long, global = true, value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,
}

#[derive(Copy, Clone, Debug, ValueEnum)] pub enum LogFormat { Text, Json }

#[derive(Subcommand, Debug)]
pub enum Cmd {
    Template { #[command(subcommand)] action: TemplateCmd },
    Launch(LaunchArgs),
    List(ListArgs),
    Connect(ConnectArgs),
    Terminate(TerminateArgs),
    Config { #[command(subcommand)] action: ConfigCmd },
}

#[derive(Subcommand, Debug)]
pub enum TemplateCmd {
    Create(TemplateCreateArgs),
    Update(TemplateCreateArgs),
    List,
    Delete { name: String, #[arg(long)] yes: bool },
}

#[derive(Args, Debug)]
pub struct TemplateCreateArgs {
    #[arg(long)] pub name: String,
    #[arg(long)] pub ami: Option<String>,
    #[arg(long)] pub instance_type: Option<String>,
    #[arg(long)] pub key_name: Option<String>,
    #[arg(long)] pub user_data: Option<String>,
}

#[derive(Args, Debug)]
pub struct LaunchArgs {
    #[arg(long)] pub template: Option<String>,
    #[arg(long)] pub name: Option<String>,
    #[arg(long)] pub instance_type: Option<String>,
    #[arg(long, value_enum)] pub market: Option<Market>,
    #[arg(long)] pub connect: bool,
    #[arg(long)] pub wait_bootstrap: bool,
    #[arg(long)] pub dry_run: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)] pub enum Market { Spot, OnDemand }

#[derive(Args, Debug)]
pub struct ListArgs { #[arg(long)] pub json: bool }

#[derive(Args, Debug)]
pub struct ConnectArgs {
    pub name: String,
    #[arg(long)] pub print: bool,
    #[arg(long)] pub no_tmux: bool,
    #[arg(long)] pub tmux_session: Option<String>,
    #[arg(long, name = "instance-id")] pub instance_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct TerminateArgs { pub name: String, #[arg(long)] pub yes: bool }

#[derive(Subcommand, Debug)]
pub enum ConfigCmd { Show, Path }
```

- [ ] **Step 5: `dispatch.rs`**

```rust
use std::sync::Arc;
use std::os::unix::process::CommandExt as _;
use std::process::Command;

use kleya_core::Config;
use kleya_core::commands::{
    connect::{ConnectOpts, ConnectService},
    launch::{LaunchOpts, LaunchService},
    list::ListService,
    template::TemplateService,
    terminate::TerminateService,
};
use kleya_core::ports::id_gen::AdjAnimalIdGen;

use crate::clap_args::{Cli, Cmd, TemplateCmd, ConfigCmd};
use crate::config_loader;
use crate::key_store_fs::FsKeyStore;

pub async fn run(cli: Cli) -> kleya_core::Result<()> {
    let config = Arc::new(config_loader::load(cli.config.as_deref())?);
    let region = cli.region.clone().unwrap_or_else(|| config.default_region.clone());
    let ec2 = kleya_aws::client::build_ec2_client(&region, None).await;
    let ssm = {
        use aws_config::BehaviorVersion;
        let cfg = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_ec2::config::Region::new(region.clone()))
            .load().await;
        aws_sdk_ssm::Client::new(&cfg)
    };
    let compute = Arc::new(kleya_aws::ec2::AwsEc2 {
        ec2: Arc::new(ec2), ssm: Arc::new(ssm), region: region.clone(),
    });
    let key_store = Arc::new(FsKeyStore::from_config(&config.keys)?);

    match cli.command {
        Cmd::Template { action } => match action {
            TemplateCmd::Create(_) | TemplateCmd::Update(_) =>
                Err(kleya_core::Error::ConfigInvalid { reason: "template create/update via CLI deferred to launch flow".into() }),
            TemplateCmd::List => {
                let svc = TemplateService { compute: compute.clone(), config: config.clone() };
                for t in svc.list().await? { println!("{}\t{}\tv{}", t.id.0, t.name.0, t.latest_version.0); }
                Ok(())
            }
            TemplateCmd::Delete { name, .. } => {
                TemplateService { compute, config }
                    .delete_by_name(&kleya_core::model::template::TemplateName(name)).await
            }
        },
        Cmd::Launch(args) => {
            let svc = LaunchService {
                compute, key_store, id_gen: Arc::new(AdjAnimalIdGen),
                config, bootstrap_tpl: kleya_bootstrap_assets::SETUP_TEMPLATE,
                ghostty_tinfo: kleya_bootstrap_assets::GHOSTTY_TERMINFO,
            };
            let res = svc.run(LaunchOpts {
                template_name: args.template, instance_name: args.name, dry_run: args.dry_run,
            }).await?;
            if let Some(inst) = &res {
                println!("launched: id={} name={} dns={}",
                    inst.id.as_str(),
                    inst.name.as_ref().map(|n| n.as_str()).unwrap_or("-"),
                    inst.public_dns.as_deref().unwrap_or("-"));
            }
            Ok(())
        }
        Cmd::List(args) => {
            let list = ListService { compute }.list_managed().await?;
            if args.json {
                let json = serde_json::to_string_pretty(&list).map_err(|e| kleya_core::Error::Io(
                    std::io::Error::new(std::io::ErrorKind::Other, e)))?;
                println!("{json}");
            } else {
                for i in list {
                    println!("{}\t{}\t{:?}\t{}",
                        i.id.as_str(),
                        i.name.as_ref().map(|n| n.as_str()).unwrap_or("-"),
                        i.state,
                        i.public_dns.unwrap_or_else(|| "-".into()));
                }
            }
            Ok(())
        }
        Cmd::Connect(args) => {
            let svc = ConnectService {
                compute, key_store, config, region,
            };
            let plan = svc.plan(&ConnectOpts {
                handle: args.name, explicit_instance_id: args.instance_id,
                no_tmux: args.no_tmux, tmux_session: args.tmux_session,
            }).await?;
            if args.print {
                println!("{}", shell_quote(&plan.argv));
                return Ok(());
            }
            let err = Command::new(&plan.argv[0]).args(&plan.argv[1..]).exec();
            Err(kleya_core::Error::Io(err))
        }
        Cmd::Terminate(args) => {
            TerminateService { compute, region: region.clone() }
                .terminate_by_handle(&args.name).await.map(|_| ())
        }
        Cmd::Config { action } => match action {
            ConfigCmd::Show => {
                let s = toml::to_string_pretty(&*config).map_err(|e| kleya_core::Error::Io(
                    std::io::Error::new(std::io::ErrorKind::Other, e)))?;
                println!("{s}"); Ok(())
            }
            ConfigCmd::Path => {
                println!("{}", config_loader::resolved_path(cli.config.as_deref())
                    .unwrap_or_else(|| "<defaults; no file loaded>".into()));
                Ok(())
            }
        }
    }
}

fn shell_quote(argv: &[String]) -> String {
    argv.iter().map(|s| if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_/.@=:".contains(c)) {
        s.clone()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }).collect::<Vec<_>>().join(" ")
}
```

- [ ] **Step 6: `main.rs`**

```rust
use clap::Parser as _;
use std::process::ExitCode;

mod clap_args;
mod config_loader;
mod dispatch;
mod exit_code;
mod key_store_fs;
mod logging;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = clap_args::Cli::parse();
    logging::init(cli.verbose, matches!(cli.log_format, clap_args::LogFormat::Json));
    match dispatch::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e);
            ExitCode::from(u8::try_from(exit_code::code_for(&e)).unwrap_or(1))
        }
    }
}
```

- [ ] **Step 7: Stub `config_loader.rs` and `key_store_fs.rs` (full impl in Tasks 19–20)**

`config_loader.rs`:
```rust
use kleya_core::{Config, Result};

pub fn load(path: Option<&str>) -> Result<Config> {
    let _ = path;
    let cfg = Config::default(); cfg.validate()?; Ok(cfg)
}

#[must_use]
pub fn resolved_path(path: Option<&str>) -> Option<String> {
    path.map(str::to_string)
}
```

`key_store_fs.rs`:
```rust
use std::path::PathBuf;
use kleya_core::{
    Result,
    model::key::{Fingerprint, KeyName, KeyPair, PublicKey},
    ports::key_store::KeyStore,
    config::KeysCfg,
};

pub struct FsKeyStore { pub dir: PathBuf }

impl FsKeyStore {
    pub fn from_config(_cfg: &KeysCfg) -> Result<Self> {
        Ok(Self { dir: PathBuf::from(shellexpand::tilde("~/.config/kleya/keys").to_string()) })
    }
}

impl KeyStore for FsKeyStore {
    fn ensure_dir(&self) -> Result<PathBuf>  { Ok(self.dir.clone()) }
    fn generate(&self, _name: &KeyName) -> Result<KeyPair> {
        Err(kleya_core::Error::ConfigInvalid { reason: "FsKeyStore::generate not yet implemented (Task 20)".into() })
    }
    fn read_public(&self, _name: &KeyName) -> Result<PublicKey> {
        Err(kleya_core::Error::ConfigInvalid { reason: "FsKeyStore::read_public not yet implemented (Task 20)".into() })
    }
    fn private_path(&self, name: &KeyName) -> Result<PathBuf> {
        Ok(self.dir.join(format!("{name}.pem")))
    }
    fn exists(&self, name: &KeyName) -> bool { self.dir.join(format!("{name}.pem")).exists() }
    fn delete(&self, _name: &KeyName) -> Result<()> { Ok(()) }
    fn fingerprint(&self, _name: &KeyName) -> Result<Fingerprint> {
        Err(kleya_core::Error::ConfigInvalid { reason: "FsKeyStore::fingerprint not yet implemented (Task 20)".into() })
    }
}
```

Add `shellexpand = "3"` to `kleya-cli/Cargo.toml`.

- [ ] **Step 8: Verify**

```bash
cargo check -p kleya-cli
cargo clippy -p kleya-cli --all-targets -- -D warnings
cargo run -p kleya-cli -- --help
```
Expected: PASS; `--help` lists all subcommands.

---

## Task 19: Multi-format config loader (TOML/YAML/JSON/JSONC) + property test

**Files:**
- Modify: `crates/kleya-cli/src/config_loader.rs`
- Create: `crates/kleya-cli/tests/config_roundtrip.rs`

- [ ] **Step 1: Implement loader**

```rust
use std::fs;
use std::path::{Path, PathBuf};

use kleya_core::{Config, Error, Result};
use kleya_core::limits::CONFIG_BYTES_MAX;

pub fn load(explicit: Option<&str>) -> Result<Config> {
    let path = resolved_path(explicit);
    let Some(path) = path else {
        let cfg = Config::default(); cfg.validate()?; return Ok(cfg);
    };
    let p = PathBuf::from(shellexpand::tilde(&path).to_string());
    let bytes = fs::read(&p)?;
    if bytes.len() > CONFIG_BYTES_MAX {
        return Err(Error::ConfigInvalid {
            reason: format!("config file {} bytes > {CONFIG_BYTES_MAX}", bytes.len()),
        });
    }
    let text = String::from_utf8(bytes).map_err(|e| Error::ConfigInvalid {
        reason: format!("config not utf-8: {e}"),
    })?;
    let cfg = parse_by_ext(&p, &text)?;
    cfg.validate()?;
    Ok(cfg)
}

fn parse_by_ext(path: &Path, text: &str) -> Result<Config> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "toml" => toml::from_str(text).map_err(map_serde),
        "yaml" | "yml" => serde_yaml::from_str(text).map_err(map_serde),
        "json" => serde_json::from_str(text).map_err(map_serde),
        "jsonc" => {
            let json = jsonc_parser::parse_to_serde_value(text, &Default::default())
                .map_err(|e| Error::ConfigInvalid { reason: format!("jsonc: {e}") })?
                .ok_or_else(|| Error::ConfigInvalid { reason: "jsonc empty".into() })?;
            serde_json::from_value(json).map_err(map_serde)
        }
        other => Err(Error::ConfigInvalid { reason: format!("unknown config extension: {other}") }),
    }
}

fn map_serde<E: std::fmt::Display>(e: E) -> Error {
    Error::ConfigInvalid { reason: format!("parse: {e}") }
}

#[must_use]
pub fn resolved_path(explicit: Option<&str>) -> Option<String> {
    if let Some(p) = explicit { return Some(p.to_string()); }
    let home = std::env::var("HOME").ok()?;
    for ext in ["toml", "yaml", "yml", "json", "jsonc"] {
        let p = format!("{home}/.config/kleya/config.{ext}");
        if std::path::Path::new(&p).exists() { return Some(p); }
    }
    None
}
```

- [ ] **Step 2: Property + golden round-trip test**

`crates/kleya-cli/tests/config_roundtrip.rs`:
```rust
use kleya_core::Config;

#[test]
fn defaults_serialize_in_all_formats_and_reparse_equal() {
    let cfg = Config::default();
    let toml_text = toml::to_string(&cfg).expect("toml");
    let yaml_text = serde_yaml::to_string(&cfg).expect("yaml");
    let json_text = serde_json::to_string(&cfg).expect("json");

    let toml_back: Config = toml::from_str(&toml_text).expect("toml parse");
    let yaml_back: Config = serde_yaml::from_str(&yaml_text).expect("yaml parse");
    let json_back: Config = serde_json::from_str(&json_text).expect("json parse");

    assert_eq!(cfg, toml_back);
    assert_eq!(cfg, yaml_back);
    assert_eq!(cfg, json_back);
}

#[test]
fn jsonc_with_comments_parses() {
    let jsonc = r#"
        {
            // comment
            "default_region": "us-east-1",
            "default_profile": "default"
        }
    "#;
    let v = jsonc_parser::parse_to_serde_value(jsonc, &Default::default())
        .expect("jsonc").expect("value");
    let c: Config = serde_json::from_value(v).expect("config");
    assert_eq!(c.default_region, "us-east-1");
}
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-cli
```
Expected: PASS.

---

## Task 20: Filesystem `KeyStore` with Ed25519 + permission assertions + EC2-style fingerprint

**Files:**
- Modify: `crates/kleya-cli/src/key_store_fs.rs`
- Modify: `crates/kleya-cli/Cargo.toml` (ensure `md-5`, `hex`, `base64` are listed under `[dependencies]`)

Add to `crates/kleya-cli/Cargo.toml` under `[dependencies]` (most already present from Task 18; add `base64` if not):
```toml
md-5   = "0.10"
hex    = "0.4"
base64 = { workspace = true }
```

- [ ] **Step 1: Implement Ed25519 generate + read + permission checks + fingerprint**

```rust
use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use md5::{Digest, Md5};

use kleya_core::{
    Error, Result,
    config::KeysCfg,
    model::key::{Fingerprint, KeyName, KeyPair, PublicKey},
    ports::key_store::KeyStore,
};
use ssh_key::{Algorithm, PrivateKey, rand_core::OsRng};

const DIR_MODE:  u32 = 0o700;
const FILE_MODE: u32 = 0o600;

pub struct FsKeyStore { dir: PathBuf, default_key: String }

impl FsKeyStore {
    pub fn from_config(cfg: &KeysCfg) -> Result<Self> {
        let dir = PathBuf::from(shellexpand::tilde(&cfg.dir).to_string());
        Ok(Self { dir, default_key: cfg.default_key_name.clone() })
    }
    #[must_use] pub fn default_key_name(&self) -> &str { &self.default_key }

    fn path_for(&self, name: &KeyName) -> PathBuf { self.dir.join(format!("{name}.pem")) }

    fn assert_dir_mode(&self) -> Result<()> {
        let md = fs::metadata(&self.dir)?;
        let mode = md.permissions().mode() & 0o777;
        if mode != DIR_MODE {
            return Err(Error::ConfigInvalid {
                reason: format!("{} mode is {mode:o} not {DIR_MODE:o}", self.dir.display()),
            });
        }
        Ok(())
    }

    fn assert_file_mode(&self, p: &PathBuf) -> Result<()> {
        let md = fs::metadata(p)?;
        let mode = md.permissions().mode() & 0o777;
        if mode != FILE_MODE {
            return Err(Error::ConfigInvalid {
                reason: format!("{} mode is {mode:o} not {FILE_MODE:o}", p.display()),
            });
        }
        Ok(())
    }
}

impl KeyStore for FsKeyStore {
    fn ensure_dir(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        fs::set_permissions(&self.dir, fs::Permissions::from_mode(DIR_MODE))?;
        self.assert_dir_mode()?;
        Ok(self.dir.clone())
    }

    fn generate(&self, name: &KeyName) -> Result<KeyPair> {
        self.ensure_dir()?;
        let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
            .map_err(|e| Error::ConfigInvalid { reason: format!("ed25519: {e}") })?;
        let private = key.to_openssh(ssh_key::LineEnding::LF)
            .map_err(|e| Error::ConfigInvalid { reason: format!("openssh: {e}") })?;
        let public  = key.public_key().to_openssh()
            .map_err(|e| Error::ConfigInvalid { reason: format!("openssh: {e}") })?;
        let path = self.path_for(name);
        let mut f = fs::OpenOptions::new()
            .create_new(true).write(true).mode(FILE_MODE).open(&path)?;
        f.write_all(private.as_bytes())?;
        self.assert_file_mode(&path)?;
        Ok(KeyPair {
            name: name.clone(),
            public:  PublicKey(public),
            private: private.to_string(),
        })
    }

    fn read_public(&self, name: &KeyName) -> Result<PublicKey> {
        let path = self.path_for(name);
        self.assert_file_mode(&path)?;
        let text = fs::read_to_string(&path)?;
        let key = PrivateKey::from_openssh(&text)
            .map_err(|e| Error::ConfigInvalid { reason: format!("openssh: {e}") })?;
        let pub_text = key.public_key().to_openssh()
            .map_err(|e| Error::ConfigInvalid { reason: format!("openssh: {e}") })?;
        Ok(PublicKey(pub_text))
    }

    fn private_path(&self, name: &KeyName) -> Result<PathBuf> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(Error::KeyOrphaned { name: name.to_string() });
        }
        self.assert_file_mode(&path)?;
        Ok(path)
    }

    fn exists(&self, name: &KeyName) -> bool { self.path_for(name).exists() }
    fn delete(&self, name: &KeyName) -> Result<()> {
        let p = self.path_for(name);
        if p.exists() { fs::remove_file(p)?; }
        Ok(())
    }

    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint> {
        let pub_text = self.read_public(name)?;
        let b64 = pub_text.0.split_whitespace().nth(1).ok_or_else(||
            Error::ConfigInvalid { reason: format!("malformed openssh public key for {name}") })?;
        let raw = B64.decode(b64).map_err(|e|
            Error::ConfigInvalid { reason: format!("base64 decode for {name}: {e}") })?;
        assert!(!raw.is_empty(), "decoded key body empty");
        let digest = Md5::digest(&raw);
        let hexstr = hex::encode(digest);
        assert_eq!(hexstr.len(), 32, "md5 hex is 32 chars");
        // Insert ':' between every byte: aa:bb:cc:...
        let mut out = String::with_capacity(47);
        for (i, c) in hexstr.chars().enumerate() {
            if i > 0 && i % 2 == 0 { out.push(':'); }
            out.push(c);
        }
        assert_eq!(out.len(), 47, "colon-formatted fingerprint is 47 chars");
        Ok(Fingerprint(out))
    }
}
```

- [ ] **Step 1b: Tests for fingerprint determinism and format**

Append to `crates/kleya-cli/tests/key_store_fs.rs`:
```rust
#[test]
fn fingerprint_is_deterministic_and_well_formatted() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = kleya_core::config::KeysCfg {
        dir: tmp.path().display().to_string(),
        default_key_name: "k".into(),
    };
    let store = kleya_cli::key_store_fs::FsKeyStore::from_config(&cfg).unwrap();
    let name = kleya_core::model::key::KeyName::new("k").unwrap();
    store.generate(&name).unwrap();
    let fp1 = store.fingerprint(&name).unwrap().0;
    let fp2 = store.fingerprint(&name).unwrap().0;
    assert_eq!(fp1, fp2, "fingerprint must be deterministic");
    assert_eq!(fp1.len(), 47, "aa:bb:... format is 47 chars");
    assert!(fp1.chars().filter(|c| *c == ':').count() == 15);
}
```

- [ ] **Step 2: Add tempdir-based tests**

Add to `crates/kleya-cli/Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

`crates/kleya-cli/tests/key_store_fs.rs`:
```rust
use kleya_core::config::KeysCfg;
use kleya_core::model::key::KeyName;
use kleya_core::ports::key_store::KeyStore;

#[test]
fn generate_then_read_and_path() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = KeysCfg { dir: tmp.path().display().to_string(), default_key_name: "k".into() };
    let store = kleya_cli::key_store_fs::FsKeyStore::from_config(&cfg).unwrap();
    let name = KeyName::new("k").unwrap();
    store.generate(&name).expect("generate");
    let _pub_text = store.read_public(&name).expect("read");
    let path = store.private_path(&name).expect("path");
    assert!(path.exists());
}

#[test]
fn private_path_errors_when_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = KeysCfg { dir: tmp.path().display().to_string(), default_key_name: "k".into() };
    let store = kleya_cli::key_store_fs::FsKeyStore::from_config(&cfg).unwrap();
    let name = KeyName::new("absent").unwrap();
    let err = store.private_path(&name).unwrap_err();
    assert!(matches!(err, kleya_core::Error::KeyOrphaned { .. }));
}
```

Expose `key_store_fs` in `kleya-cli/src/main.rs` by making it `pub mod` for tests, OR add a `lib.rs` to `kleya-cli` that exposes these modules. Take the latter approach: add `crates/kleya-cli/src/lib.rs`:
```rust
pub mod clap_args;
pub mod config_loader;
pub mod dispatch;
pub mod exit_code;
pub mod key_store_fs;
pub mod logging;
```
And in `main.rs` replace the module declarations with `use kleya_cli::*;`.

In `crates/kleya-cli/Cargo.toml`:
```toml
[lib]
path = "src/lib.rs"
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-cli
```
Expected: PASS.

---

## Task 21: CLI smoke test against fake adapter (no AWS, no Floci)

**Files:**
- Modify: `crates/kleya-cli/src/dispatch.rs` to accept an injected `CloudCompute` (refactor for testability)
- Create: `crates/kleya-cli/tests/cli_smoke.rs`

- [ ] **Step 1: Refactor `dispatch::run` to take a builder**

Replace the entire contents of `crates/kleya-cli/src/dispatch.rs` with the following. The match arms are the same as Task 18; what changed is that all the wiring is now in `run`, and `run_with` only takes the four resolved pieces (config + region + compute + key_store) as parameters so tests can substitute fakes.

```rust
use std::os::unix::process::CommandExt as _;
use std::process::Command;
use std::sync::Arc;

use kleya_core::Config;
use kleya_core::commands::{
    connect::{ConnectOpts, ConnectService},
    launch::{LaunchOpts, LaunchService},
    list::ListService,
    template::TemplateService,
    terminate::TerminateService,
};
use kleya_core::ports::cloud_compute::CloudCompute;
use kleya_core::ports::id_gen::AdjAnimalIdGen;
use kleya_core::ports::key_store::KeyStore;

use crate::clap_args::{Cli, Cmd, ConfigCmd, TemplateCmd};
use crate::config_loader;
use crate::key_store_fs::FsKeyStore;

pub async fn run(cli: Cli) -> kleya_core::Result<()> {
    let config = Arc::new(config_loader::load(cli.config.as_deref())?);
    let region = cli.region.clone().unwrap_or_else(|| config.default_region.clone());
    let ec2 = kleya_aws::client::build_ec2_client(&region, None).await;
    let ssm = {
        use aws_config::BehaviorVersion;
        let cfg = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_ec2::config::Region::new(region.clone()))
            .load().await;
        aws_sdk_ssm::Client::new(&cfg)
    };
    let compute: Arc<dyn CloudCompute> = Arc::new(kleya_aws::ec2::AwsEc2 {
        ec2: Arc::new(ec2), ssm: Arc::new(ssm), region: region.clone(),
    });
    let key_store: Arc<dyn KeyStore> = Arc::new(FsKeyStore::from_config(&config.keys)?);
    run_with(cli, config, region, compute, key_store).await
}

pub async fn run_with(
    cli: Cli,
    config: Arc<Config>,
    region: String,
    compute: Arc<dyn CloudCompute>,
    key_store: Arc<dyn KeyStore>,
) -> kleya_core::Result<()> {
    match cli.command {
        Cmd::Template { action } => match action {
            TemplateCmd::Create(_) | TemplateCmd::Update(_) => {
                Err(kleya_core::Error::ConfigInvalid {
                    reason: "template create/update via CLI deferred to launch flow".into(),
                })
            }
            TemplateCmd::List => {
                let svc = TemplateService { compute: compute.clone(), config: config.clone() };
                for t in svc.list().await? {
                    println!("{}\t{}\tv{}", t.id.0, t.name.0, t.latest_version.0);
                }
                Ok(())
            }
            TemplateCmd::Delete { name, .. } => {
                TemplateService { compute, config }
                    .delete_by_name(&kleya_core::model::template::TemplateName(name)).await
            }
        },
        Cmd::Launch(args) => {
            let svc = LaunchService {
                compute,
                key_store,
                id_gen: Arc::new(AdjAnimalIdGen),
                config,
                bootstrap_tpl: kleya_bootstrap_assets::SETUP_TEMPLATE,
                ghostty_tinfo: kleya_bootstrap_assets::GHOSTTY_TERMINFO,
            };
            let res = svc.run(LaunchOpts {
                template_name: args.template,
                instance_name: args.name,
                dry_run: args.dry_run,
            }).await?;
            if let Some(inst) = &res {
                println!(
                    "launched: id={} name={} dns={}",
                    inst.id.as_str(),
                    inst.name.as_ref().map(|n| n.as_str()).unwrap_or("-"),
                    inst.public_dns.as_deref().unwrap_or("-"),
                );
            }
            Ok(())
        }
        Cmd::List(args) => {
            let list = ListService { compute }.list_managed().await?;
            if args.json {
                let json = serde_json::to_string_pretty(&list).map_err(|e|
                    kleya_core::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
                println!("{json}");
            } else {
                for i in list {
                    println!("{}\t{}\t{:?}\t{}",
                        i.id.as_str(),
                        i.name.as_ref().map(|n| n.as_str()).unwrap_or("-"),
                        i.state,
                        i.public_dns.unwrap_or_else(|| "-".into()));
                }
            }
            Ok(())
        }
        Cmd::Connect(args) => {
            let svc = ConnectService { compute, key_store, config, region };
            let plan = svc.plan(&ConnectOpts {
                handle: args.name,
                explicit_instance_id: args.instance_id,
                no_tmux: args.no_tmux,
                tmux_session: args.tmux_session,
            }).await?;
            if args.print {
                println!("{}", shell_quote(&plan.argv));
                return Ok(());
            }
            let err = Command::new(&plan.argv[0]).args(&plan.argv[1..]).exec();
            Err(kleya_core::Error::Io(err))
        }
        Cmd::Terminate(args) => {
            TerminateService { compute, region: region.clone() }
                .terminate_by_handle(&args.name).await.map(|_| ())
        }
        Cmd::Config { action } => match action {
            ConfigCmd::Show => {
                let s = toml::to_string_pretty(&*config).map_err(|e|
                    kleya_core::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
                println!("{s}");
                Ok(())
            }
            ConfigCmd::Path => {
                println!("{}", config_loader::resolved_path(cli.config.as_deref())
                    .unwrap_or_else(|| "<defaults; no file loaded>".into()));
                Ok(())
            }
        },
    }
}

fn shell_quote(argv: &[String]) -> String {
    argv.iter().map(|s| {
        if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_/.@=:".contains(c)) {
            s.clone()
        } else {
            format!("'{}'", s.replace('\'', r"'\''"))
        }
    }).collect::<Vec<_>>().join(" ")
}
```

No `todo!()` should remain in committed source.

- [ ] **Step 2: Smoke test**

`crates/kleya-cli/tests/cli_smoke.rs`:
```rust
use std::sync::Arc;
use kleya_core::test_support::{InMemoryCompute, InMemoryKeyStore};
use kleya_cli::clap_args::*;
use clap::Parser as _;

#[tokio::test]
async fn list_subcommand_runs_against_fake_with_no_instances() {
    let cli = Cli::parse_from(["kleya", "list"]);
    let cfg = Arc::new(kleya_core::Config::default());
    let compute: Arc<dyn kleya_core::ports::cloud_compute::CloudCompute> =
        Arc::new(InMemoryCompute::new());
    let key_store: Arc<dyn kleya_core::ports::key_store::KeyStore> =
        Arc::new(InMemoryKeyStore::new());
    kleya_cli::dispatch::run_with(cli, cfg, "eu-west-1".into(), compute, key_store)
        .await.expect("ok");
}
```

- [ ] **Step 3: Verify**

```bash
cargo nextest run -p kleya-cli
```
Expected: PASS.

---

## Task 22: lefthook, cargo-deny, GitHub Actions CI

**Files:**
- Create: `lefthook.yml`
- Create: `deny.toml`
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: `lefthook.yml`**

```yaml
pre-push:
  parallel: false
  commands:
    fmt:
      run: cargo fmt --all -- --check
    clippy:
      run: cargo clippy --workspace --all-targets --all-features -- -D warnings
    test:
      run: cargo nextest run --workspace --no-fail-fast
```

Install hook:
```bash
lefthook install
```

- [ ] **Step 2: `deny.toml`**

```toml
[graph]
all-features = true

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-DFS-2016", "MPL-2.0"]
confidence-threshold = 0.93

[bans]
multiple-versions = "warn"

[advisories]
yanked = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

- [ ] **Step 3: `.github/workflows/ci.yml`**

```yaml
name: ci
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.95.0
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-nextest --locked
      - run: cargo install cargo-deny --locked
      - run: cargo install cargo-llvm-cov --locked
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      - run: cargo nextest run --workspace
      - run: cargo deny check
      - run: cargo llvm-cov --workspace --fail-under-lines 50

  floci:
    runs-on: ubuntu-latest
    services:
      floci:
        image: floci/floci:latest
        ports: [4566:4566]
        options: --health-cmd "wget -qO- http://localhost:4566/_floci/health" --health-interval 5s
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { toolchain: 1.95.0 }
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-nextest --locked
      - run: KLEYA_TEST_FLOCI=1 KLEYA_TEST_FLOCI_ENDPOINT=http://localhost:4566 \
                cargo nextest run -p kleya-aws --run-ignored
```

- [ ] **Step 4: Verify pre-push runs locally**

```bash
lefthook run pre-push
```
Expected: PASS or output explaining the failing check.

---

## Task 23: Final integration sanity & manual smoke

- [ ] **Step 1: Build + tests**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
```
Expected: PASS.

- [ ] **Step 2: `--help` smoke**

```bash
cargo run -p kleya-cli -- --help
cargo run -p kleya-cli -- launch --help
cargo run -p kleya-cli -- connect --help
```
Expected: command tree shown, no panic.

- [ ] **Step 3: Dry-run launch (no AWS calls)**

```bash
AWS_REGION=eu-west-1 cargo run -p kleya-cli -- launch --dry-run
```
Expected: prints a resolved plan; exits 0. (Will hit AWS for SSM AMI resolution unless mocked — acceptable on a real machine with creds. If no creds, an Adapter error with exit code 70 is the expected failure mode.)

---

## Self-Review

**Spec coverage check** (against `docs/superpowers/specs/2026-05-16-kleya-bootstrap-design.md`):

| Spec section | Task(s) |
|---|---|
| §2 Workspace layout | Task 1 |
| §3 Provider port | Task 6 |
| §4 CLI surface | Task 18 |
| §5 Configuration + multi-format | Task 5, 19 |
| §6 Zero-config defaults + ensure-* flow | Task 12, 16 |
| §7 Bootstrap rendering + ghostty | Task 7, 8, 9 |
| §8 Connect flow + tag-based key lookup | Task 14, 18 |
| §9 Key management (Ed25519, modes) | Task 20 |
| §10 Error model + exit codes | Task 3, 18 |
| §11 Named limits | Task 2 |
| §12 Testing (fakes + Floci tiers) | Task 10, 11, 17 |
| §13 Telemetry / logging | Task 18 |
| §14 Runtime / current-thread / signal | Task 18 (`#[tokio::main(flavor = "current_thread")]`) |
| §15 Hygiene (lefthook, deny, CI) | Task 22 |

**Placeholder scan:** the `todo!()` in Task 21 Step 1 is annotated as a "shown for clarity, real implementation pastes the body from Task 18" — replace it during implementation. No other `TBD`/`TODO` markers remain.

**Type consistency:** `CloudCompute` signatures in Task 6 are referenced unchanged in Tasks 10, 11, 12, 13, 14, 15, 16. `KeyStore` signatures in Task 6 used unchanged in Tasks 10, 20. `TemplateSpec`/`LaunchRequest`/`Instance` field names match across all consuming tasks.

---

## Execution Handoff

**Plan complete.** Saved to `docs/superpowers/plans/2026-05-16-kleya-bootstrap.md`.

Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using executing-plans with checkpoints.

Which approach?

