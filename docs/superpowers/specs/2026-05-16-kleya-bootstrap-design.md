# `kleya` — Design Spec

- **Date:** 2026-05-16
- **Status:** Draft, pending review
- **Scope:** A small Rust CLI that bootstraps AWS spot instances for agentic coding environments, with a port/adapter architecture so non-AWS providers can be added later.

## 1. Purpose

Replace the existing `~/boxes/devbox-ie/launch.sh` shell pipeline with a typed, testable, multi-provider Rust CLI named `kleya`. The CLI must:

- Create, update, list, and delete EC2 launch templates.
- Launch spot instances from a template, tagged so they can be referenced by a human-readable name.
- Bootstrap each instance with a Claude-Code-ready development environment (zsh, gh, tmux, rust, node, jj, python, uv, Claude Code) per the `setup_devbox.sh` gist, including server-side `xterm-ghostty` terminfo installation.
- Connect to an instance over SSH and attach to a tmux session.
- Terminate instances.
- Work zero-config: `kleya launch` with no flags or config file launches a working dev box.

Accept either CLI flags or a config file (TOML, YAML, JSON, or JSONC); CLI flags override file values.

Development follows the strict Tiger-style guidelines (see `08 — Development Guidelines`): defensive coding, named limits, two-assertion minimum per function, no `unwrap`/`expect` in production paths, 70-line function cap, 100-column line cap, `cargo-nextest` for tests, jj for VCS.

## 2. Workspace Layout

```
kleya-bootstrap/
  Cargo.toml                       # workspace
  rust-toolchain.toml              # pinned to stable 1.95.0
  clippy.toml                      # pedantic-adjacent lints
  crates/
    kleya-core/                    # domain + ports; no I/O, no provider SDKs
      src/lib.rs                   # crate doc: purpose, ports, surface
      src/error.rs                 # one Error enum (thiserror)
      src/limits.rs                # named bounds — every magic number lives here
      src/model/                   # InstanceSpec, TemplateSpec, Instance, Tag, KeyName, InstanceId, …
      src/ports/                   # trait CloudCompute, trait KeyStore, trait Clock, trait IdGen
      src/commands/                # one file per subcommand (orchestration only)
      src/bootstrap/               # pure renderer for the user-data script
    kleya-aws/                     # adapter — implements CloudCompute for EC2 via aws-sdk-ec2
      src/lib.rs
      src/ec2.rs                   # mapping between core types and aws_sdk_ec2 types
      src/error.rs                 # adapter-local error -> core::Error::Adapter
    kleya-cli/                     # the `kleya` binary
      src/main.rs                  # ~10 lines: parse → build adapters → dispatch
      src/clap_args.rs             # clap derive types only
      src/config_loader.rs         # multi-format loader → kleya_core::Config
      src/key_store_fs.rs          # filesystem KeyStore impl (local host I/O)
    kleya-bootstrap-assets/        # embeds the user-data script + ghostty.terminfo
      src/lib.rs
      assets/setup_devbox.sh.j2
      assets/ghostty.terminfo
  docs/superpowers/specs/          # this design doc
```

**Why a workspace, not a single crate:** the user intends to add non-AWS providers (GCP, Hetzner, etc.). Putting `CloudCompute` in `kleya-core` and the EC2 implementation in `kleya-aws` enforces the port/adapter boundary at the crate level. Adding a provider is a new crate that depends only on `kleya-core`. `kleya-core` never depends on `aws-sdk-ec2`.

## 3. Provider Port

```rust
// crates/kleya-core/src/ports/cloud_compute.rs
#[async_trait]
pub trait CloudCompute: Send + Sync {
    async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId>;
    async fn template_update(&self, id: &TemplateId, spec: &TemplateSpec) -> Result<TemplateVersion>;
    async fn template_list(&self) -> Result<Vec<TemplateSummary>>;
    async fn template_delete(&self, id: &TemplateId) -> Result<()>;

    async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance>;
    async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>>;
    async fn instance_describe(&self, id: &InstanceId) -> Result<Instance>;
    async fn instance_terminate(&self, id: &InstanceId) -> Result<()>;
    async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance>;

    // Default-resource lifecycle (idempotent; called by the zero-config path)
    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId>;
    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()>;
    async fn ensure_default_template(&self, spec: &TemplateSpec) -> Result<TemplateId>;
    async fn resolve_default_subnet(&self) -> Result<SubnetId>;
    async fn resolve_ami_alias(&self, alias: &str) -> Result<AmiId>;
}
```

Core types (`InstanceId`, `TemplateId`, `KeyName`, etc.) are newtypes around `String` or `[u8; N]` with constructor validation — invalid values cannot be constructed, satisfying "make invalid states unrepresentable."

## 4. CLI Surface

```
kleya template create  --name <n> [--ami <id>] [--instance-type <t>] [--key-name <k>]
                       [--user-data <path>] [--region <r>]
kleya template update  <name> [same flags as create — partial update]
kleya template list    [--region <r>] [--json]
kleya template delete  <name> [--yes]

kleya launch           [--template <name>] [--name <instance-name>] [--instance-type <t>]
                       [--market spot|on-demand] [--region <r>]
                       [--connect] [--wait-bootstrap] [--dry-run]
kleya list             [--region <r>] [--json]
kleya connect          <name> [--print] [--no-tmux] [--tmux-session <s>] [--instance-id i-...]
kleya terminate        <name> [--yes] [--region <r>]

kleya config show      # resolved config after all overrides
kleya config path      # print resolved config file path
```

Global flags: `--config <path>`, `--profile <aws-profile>`, `--region <r>`, `-v/--verbose`, `--log-format json|text`.

## 5. Configuration

**Precedence (highest wins):**

1. CLI flag.
2. Environment variable (`KLEYA_*`, plus `AWS_REGION`, `AWS_PROFILE`).
3. `--config <path>` file.
4. `~/.config/kleya/config.{toml,yaml,yml,json,jsonc}` (first match wins).
5. Built-in defaults.

**Canonical TOML schema (other formats deserialize into the same struct):**

```toml
default_region = "eu-west-1"
default_profile = "default"

[defaults]
instance_type = "m8g.xlarge"
market = "spot"
spot_type = "one-time"
ami_alias = "amazon-linux-2023-arm64"

[bootstrap]
user_data_path = "~/custom-bootstrap.sh"      # optional override; else embedded asset is used
install_ghostty_terminfo = true

[ssh]
user = "ec2-user"
tmux = true
tmux_session = "kleya"
extra_args = ["-o", "ServerAliveInterval=30"]

[keys]
dir = "~/.config/kleya/keys"                  # 0700; pem files 0600

[[templates]]
name = "devbox"
ami_id = "ami-0123456789abcdef0"              # optional; resolves from alias if absent
instance_type = "m8g.xlarge"
security_group_ids = ["sg-..."]
subnet_id = "subnet-..."
key_name = "devbox"                           # CLI auto-creates if missing

[[templates.tags]]
key = "Project"
value = "kleya"
```

**Multi-format loader (`kleya-cli/src/config_loader.rs`):**

- Format chosen by file extension (`.toml`, `.yml`/`.yaml`, `.json`, `.jsonc`).
- JSONC is normalized via `jsonc-parser` → JSON → `serde_json`.
- All paths deserialize into the same `kleya_core::Config` struct, which then runs `Config::validate()` — the validation step is identical regardless of source format and is the only place where bounds and regexes are checked.
- Round-trip parity is enforced by a property test: any well-formed `Config` exported to all four formats and reloaded must equal itself.

## 6. Zero-Config Defaults

Every option has a fallback so `kleya launch` with no flags or config file launches a working dev box.

| Option | Default | Resolution |
|---|---|---|
| `region` | `eu-west-1` | env `AWS_REGION` → config → built-in |
| `profile` | `default` | env `AWS_PROFILE` → config → built-in |
| `template` | `default` (auto-created if missing) | named-template lookup; if absent, create-default path runs |
| `instance_type` | `m8g.xlarge` | matches today's `launch.sh` |
| `market` | `spot` | with `spot_type = "one-time"` |
| `ami_id` | resolved at launch | SSM public parameter `/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64` |
| `subnet_id` | first subnet of default VPC | `describe-vpcs` filter `isDefault=true` |
| `security_group_ids` | `kleya-default` SG (created if missing, 22/tcp from `0.0.0.0/0`) | port `0.0.0.0/0` is initial; will be tightened via config in a later spec |
| `key_name` | `kleya-default` (generated if missing) | Ed25519 keypair; private at `~/.config/kleya/keys/kleya-default.pem` (0600) |
| `instance name tag` | `kleya-<adjective>-<animal>` | generated; printed at launch; matches `^[a-z0-9][a-z0-9-]{0,62}$` |
| `user_data` | embedded `setup_devbox.sh` | with ghostty terminfo block enabled |
| `bootstrap.install_ghostty_terminfo` | `true` | embedded `ghostty.terminfo` is installed on the instance |
| `ssh.user` | `ec2-user` | AL2023 |
| `ssh.tmux` | `true`, session `"kleya"` | overridable per-launch |
| `ssh.extra_args` | `[]` | additional argv tokens passed verbatim to `ssh` |
| `wait_timeout` | `600 s` | named const `LAUNCH_WAIT_SECONDS_MAX` |

**Bootstrap-the-bootstrap flow** (first run on a clean account):

1. Resolve region.
2. Lookup default VPC → choose subnet via `resolve_default_subnet()`.
3. `ensure_default_security_group("kleya-default")` — idempotent.
4. `ensure_default_keypair(...)` — generates locally + imports if missing.
5. `ensure_default_template(...)` — creates the `default` template if missing.
6. `instance_launch(...)` tagged with at minimum:
   - `Name=<generated-or-supplied>`
   - `kleya:managed=true`
   - `kleya:template=<template-name>`
   - `kleya:key=<key-name>`

   plus any user-defined tags from `[[templates.tags]]`. The `kleya:template` and `kleya:key` tags are what lets `connect` resolve the right private key path without a local state file (a managed instance carries its own metadata).

Each step is a single, idempotent method on `CloudCompute`, so the orchestration is a straight-line sequence in `kleya-core/src/commands/launch.rs` — no nested conditionals.

`kleya launch --dry-run` resolves and prints the plan (region, AMI, subnet, SG, key, template) and exits 0 without provisioning.

## 7. Bootstrap (User-Data) Rendering

Embedded asset, template-rendered at launch time.

```
crates/kleya-bootstrap-assets/
  src/lib.rs
  assets/
    setup_devbox.sh.j2            # minijinja template — the gist's script with conditional blocks
    ghostty.terminfo              # version-pinned terminfo source from ghostty-org/ghostty
```

`kleya-bootstrap-assets` exposes:

```rust
pub const SETUP_TEMPLATE: &str   = include_str!("../assets/setup_devbox.sh.j2");
pub const GHOSTTY_TERMINFO: &str = include_str!("../assets/ghostty.terminfo");
```

No network fetch at runtime — terminfo and bootstrap travel inside the binary.

**Rendering** (`kleya-core/src/bootstrap/render.rs`, pure function — no I/O):

```rust
pub struct BootstrapVars<'a> {
    pub install_ghostty_terminfo: bool,
    pub ghostty_terminfo_source: &'a str,
    pub install_dev_tools: bool,
    pub node_major: u8,
    pub python_version: &'a str,
    pub extra_pre_lines: &'a [String],
    pub extra_post_lines: &'a [String],
}
pub fn render(vars: &BootstrapVars<'_>) -> Result<String>;
```

The template body is the gist's script with two changes:

1. Wrapped in `{% if install_ghostty_terminfo %} … {% endif %}` and `{% if install_dev_tools %} … {% endif %}` blocks.
2. A heredoc that writes the embedded `ghostty.terminfo` to `/tmp/ghostty.terminfo` and runs `sudo tic -x /tmp/ghostty.terminfo` so `xterm-ghostty` exists in the system terminfo database. Sketch:

```bash
{% if install_ghostty_terminfo %}
sudo dnf install -y ncurses
cat > /tmp/ghostty.terminfo <<'GHOSTTY_TERMINFO_EOF'
{{ ghostty_terminfo_source }}
GHOSTTY_TERMINFO_EOF
sudo tic -x /tmp/ghostty.terminfo
rm /tmp/ghostty.terminfo
{% endif %}
```

**Size budget:** EC2 user-data is capped at 16 KiB raw. After rendering, `kleya-core` gzip-compresses and base64-encodes the script (EC2 accepts gzip user-data — first bytes are the gzip magic). Two named consts:

- `USER_DATA_RAW_BYTES_MAX = 16_384` — asserted on rendered output before compression.
- `USER_DATA_ENCODED_BYTES_MAX = 16_384` — asserted post-encoding (EC2 enforces this on the wire).

Both checks happen in `kleya-core` before the adapter is called. Negative-space test: padding `extra_post_lines` past the limit must return `Error::UserDataTooLarge { bytes, max }`.

**Override semantics** (`bootstrap.user_data_path` in config or `--user-data <path>`):

- If override is set, read the file, **still apply the size + encoding checks**, but skip templating. The override is opaque bytes — operator's responsibility.
- `install_ghostty_terminfo` has no effect when override is in use; the CLI logs a one-line warning if both are set.

**Ghostty terminfo version pin:** the committed `ghostty.terminfo` file carries a header comment with the source commit SHA from `ghostty-org/ghostty`. Bumping it is an ordinary commit.

## 8. Connect Flow

`kleya-core/src/commands/connect.rs` orchestrates:

```
1. resolve_handle(name) → Instance         (CloudCompute::instance_list with filter on
                                            tag:Name=<name> + tag:kleya:managed=true)
2. read tags:  kleya:key → KeyName         (Instance carries its own metadata)
3. fetch_endpoint(instance) → public_dns   (already on the Instance struct from step 1)
4. resolve_key_path(key_name) → PathBuf    (KeyStore::private_path)
5. probe_ssh_ready(endpoint, deadline)     (TCP connect to 22 with backoff)
6. build_argv(endpoint, key, tmux_opts) → Vec<String>
7. if --print: print shell-quoted argv; exit 0
   else:      execvp("ssh", argv)          (replace process; clean Ctrl-C)
```

If the `kleya:key` tag is missing (e.g., instance launched out-of-band but tagged `kleya:managed=true`), fall back to `keys.default_key_name` from config. If neither resolves a name, return `Error::ConfigInvalid { reason: "no kleya:key tag on instance and no keys.default_key_name in config" }` with a remediation hint suggesting `--instance-id` and a path to a pem file or re-launching via `kleya launch`.

**Argv assembly** (one function, ~30 lines, snapshot-tested):

```
ssh -i ~/.config/kleya/keys/devbox.pem
    -o StrictHostKeyChecking=accept-new
    -o ServerAliveInterval=30
    -o ConnectTimeout=10
    [user-configured extra_args…]
    -t ec2-user@<public-dns>
    tmux new-session -A -s kleya            # only if ssh.tmux = true
```

`-A` attaches to or creates the named session. `-t` forces TTY. `--no-tmux` drops the trailing `tmux …` argv. `--tmux-session <s>` substitutes the session name (validated against `^[a-z0-9_-]{1,63}$`).

**Handle resolution outcomes** (no silent fallthrough):

- 0 matches → `Error::InstanceNotFound { name, region }`.
- 1 match → use it.
- >1 matches → `Error::AmbiguousHandle { name, candidates }`; operator passes `--instance-id i-…` to disambiguate.

`InstanceId` is also a valid handle (regex `^i-[0-9a-f]+$`); if `<name>` matches, the tag query is skipped.

**SSH readiness probe** (named consts):

- `SSH_PROBE_PORT = 22`
- `SSH_PROBE_TIMEOUT_SECONDS = 180`
- `SSH_PROBE_INTERVAL_SECONDS = 3`
- `SSH_PROBE_TCP_TIMEOUT_MS = 2_000`

TCP `connect()` with a 2 s timeout, sleep `INTERVAL`, repeat until `TIMEOUT`. Two assertions per call (`elapsed < TIMEOUT`, `interval > 0`). On exhaustion: `Error::SshNotReady { instance, elapsed_seconds }`.

**`launch` ↔ `connect` chaining:** `kleya launch` does **not** auto-attach by default — bootstrap takes minutes. Three modes:

- default: launch waits for `running`, prints `kleya connect <name>`.
- `--connect`: launch chains into `connect` after the SSH probe succeeds.
- `--wait-bootstrap`: also polls `cloud-init status --wait` over SSH before returning (best paired with `--connect`).

## 9. Key Management

**`KeyStore` port** (filesystem-only; lives in `kleya-core` as a trait, default impl in `kleya-cli/src/key_store_fs.rs`):

```rust
pub trait KeyStore: Send + Sync {
    fn ensure_dir(&self) -> Result<PathBuf>;
    fn generate(&self, name: &KeyName) -> Result<KeyPair>;
    fn read_public(&self, name: &KeyName) -> Result<PublicKey>;
    fn private_path(&self, name: &KeyName) -> Result<PathBuf>;
    fn exists(&self, name: &KeyName) -> bool;
    fn delete(&self, name: &KeyName) -> Result<()>;
}
```

Default implementation: `ssh-key` crate, **Ed25519**, OpenSSH format, `dir = ~/.config/kleya/keys`.

**`KeyName` newtype** — validated at construction (`^[a-zA-Z0-9_.-]{1,128}$`). The regex is the intersection of EC2 key-pair naming rules and POSIX-portable filename characters; reject anything else at the boundary so the `<KeyName>.pem` path is always safe to write.

**Keypair lifecycle** — no silent fallthrough:

| Local key | EC2 key registered | Action |
|---|---|---|
| present | present (fingerprints match) | use as-is |
| present | present (fingerprints differ) | `Error::KeyMismatch { name }` — refuse to overwrite |
| present | absent | `ImportKeyPair` (re-register the public half) |
| absent | present | `Error::KeyOrphaned { name }` — operator decides (`--regenerate-key` flag) |
| absent | absent | generate Ed25519 → write 0600 → `ImportKeyPair` |

Fingerprint comparison uses EC2's MD5-of-public-key value (returned by `DescribeKeyPairs`) computed locally from the stored public half. This is a paired-assertion property: wrote private + imported public ⇒ on read, fingerprints must match.

**Permissions assertions** (Unix only — Windows is out of scope; documented as such):

- Keys dir mode `0o700`, asserted at every `KeyStore` op.
- Private key mode `0o600`, asserted at read and write.
- Tested both positive (mode matches) and negative (laxer mode is rejected).

## 10. Error Model

One `Error` enum per crate, `thiserror`-derived, wrapped at boundaries.

```rust
// kleya-core/src/error.rs
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
    #[error("ssh key mismatch for {name}: local fingerprint differs from EC2 record")]
    KeyMismatch { name: KeyName },
    #[error("ssh key orphaned: {name} is in EC2 but no local private key")]
    KeyOrphaned { name: KeyName },
    #[error("adapter {provider}: {source}")]
    Adapter { provider: &'static str, #[source] source: BoxError },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

`kleya-aws::Error` is the adapter's local enum; every public method translates to `core::Error::Adapter { provider: "aws-ec2", source }` at the boundary. Core code never sees `aws_sdk_ec2::Error`. `unwrap`/`expect`/`panic!` are `#![deny]`'d via clippy in production crates; the only allowed sites are `#[cfg(test)]`.

**Exit-code mapping** (single function in `kleya-cli/src/main.rs`):

```
0   ok
2   ConfigInvalid, clap parse error                    (sysexits EX_USAGE)
3   InstanceNotFound
4   AmbiguousHandle
5   SshNotReady
6   LaunchWaitTimeout
7   KeyMismatch, KeyOrphaned
70  Adapter                                            (sysexits EX_SOFTWARE)
74  Io                                                 (sysexits EX_IOERR)
1   catch-all
```

## 11. Named Limits

All in `kleya-core/src/limits.rs`. No magic numbers elsewhere in the workspace; clippy warns on `unnamed_constant`-style usages where practical.

```rust
pub const CONFIG_BYTES_MAX: usize            = 256 * 1024;
pub const USER_DATA_RAW_BYTES_MAX: usize     = 16 * 1024;
pub const USER_DATA_ENCODED_BYTES_MAX: usize = 16 * 1024;
pub const TEMPLATES_COUNT_MAX: usize         = 64;
pub const TAGS_PER_TEMPLATE_MAX: usize       = 50;
pub const TAG_KEY_BYTES_MAX: usize           = 128;
pub const TAG_VALUE_BYTES_MAX: usize         = 256;
pub const INSTANCE_NAME_BYTES_MAX: usize     = 63;
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
```

Compile-time relationships are asserted:

```rust
const _: () = assert!(LAUNCH_POLL_INTERVAL_SECONDS as u32 <= LAUNCH_WAIT_SECONDS_MAX);
const _: () = assert!(SSH_PROBE_INTERVAL_SECONDS as u32 <= SSH_PROBE_TIMEOUT_SECONDS);
const _: () = assert!(AWS_RETRY_BACKOFF_BASE_MS <= AWS_RETRY_BACKOFF_CAP_MS);
```

Every limit boundary is tested at `value-1`, `value`, and `value+1`.

## 12. Testing Strategy

`cargo-nextest` is the only sanctioned runner.

| Tier | Scope | Location | Gating |
|---|---|---|---|
| Unit (sub-second) | `bootstrap::render`, `Config::validate`, `build_ssh_argv`, `resolve_handle` against fake, fingerprint compare, regex validation, limit boundary checks | `#[cfg(test)] mod tests` in each module | always-on |
| Integration (seconds) | Full subcommands dispatched against `InMemoryCloudCompute` and `InMemoryKeyStore`. Deterministic via `Clock` + `IdGen` fakes. Snapshot tests on rendered user-data | `crates/kleya-core/tests/`, `crates/kleya-cli/tests/` | always-on |
| AWS-shaped (Floci) | `kleya-aws` adapter against Floci EC2 — template create/list/delete, instance launch/list/terminate happy path + induced error per code path | `crates/kleya-aws/tests/floci/` | `#[ignore]` unless `KLEYA_TEST_FLOCI=1` |
| e2e (real AWS, slow) | Same harness as Floci tier but pointed at a sandbox AWS account | `crates/kleya-aws/tests/e2e/` | `#[ignore]` unless `KLEYA_TEST_E2E=1`; run on PR-to-main only |

**Floci harness:**

- `docker run --rm -p 4566:4566 -v /var/run/docker.sock:/var/run/docker.sock floci/floci@sha256:<pinned-digest>` — Docker image pinned by digest, not `:latest`.
- A small `OnceCell` wrapper starts Floci once per test binary and tears it down with a `Drop` guard. (`testcontainers-rs` is an acceptable alternative.)
- `aws_sdk_ec2::Client` is built behind `fn build_client(cfg: &AwsConfig) -> Client` which takes an optional `endpoint_url`. Production passes `None`; tests pass `Some("http://localhost:4566")` and static `test`/`test` credentials. This keeps the emulator override inside `kleya-aws`; `kleya-core` never knows the adapter is talking to an emulator.

**Property tests** (`proptest`): handle resolution and config validation are state-machine bits worth fuzzing. Required property: any well-formed `Config` round-tripped through TOML/YAML/JSON/JSONC yields the same `Config`.

**Coverage:** `cargo llvm-cov` in CI; 50% line floor per guidelines. Adapter mapping modules are excluded as mostly type-glue.

## 13. Telemetry & Logging

- `tracing` + `tracing-subscriber`. Default human-readable, level `info`. `-v` → `debug`, `-vv` → `trace`.
- `--log-format json` switches to a JSON layer; mandatory structured fields: `command`, `region`, `template`, `instance_id` where applicable.
- Secrets never logged. The SSH argv (which contains the key path) is logged at `debug` only; `KLEYA_DEBUG_SECRETS=1` is required to dump full argv at any level.
- Counter for each named limit hit emits a `tracing::warn!` event with structured `limit_hit` field — operators can grep/alert on those.

## 14. Runtime & Process Model

- `#[tokio::main(flavor = "current_thread")]` — CLI doesn't need the multi-thread scheduler.
- SIGINT during `instance_wait_running` cancels gracefully via a `CancellationToken` checked between polls. Partial work is left in AWS; operator sees it via `kleya list`.
- No recursion (guideline). All loops have explicit `attempt_count < MAX` bounds.

## 15. Repository Hygiene

- `.private/` ignored; nothing operator-specific is committed.
- jj on Git backend; `main` is integration; named bookmarks for feature work; Conventional Commits.
- `lefthook` pre-push: `cargo fmt --check`, `cargo clippy --all-targets --all-features -D warnings`, `cargo nextest run` (unit + integration tiers only).
- CI on PR-to-main: pre-push checks + `cargo nextest run --run-ignored` against Floci, `cargo deny check`, `cargo audit`, `cargo llvm-cov` with the 50% floor.

## 16. Out of Scope (v1)

- Windows support (documented).
- Multi-region orchestration in a single command (operator passes `--region`; one region per invocation).
- Spot interruption recovery / re-launch.
- Non-AWS providers (the port exists; implementations come in later specs).
- Cost reporting.
- WireGuard / mesh networking (mentioned in the broader guidelines but not in this CLI's scope).

## 17. Assumptions

- The operator has working AWS credentials (env, profile, or IMDS).
- Default VPC exists in the chosen region (zero-config path depends on it). If it doesn't, `Error::ConfigInvalid { reason: "no default VPC in region X" }` is returned with a remediation hint.
- The user's Ghostty client recognises `xterm-ghostty` locally; this design only installs it server-side.
- Docker is available for running Floci in CI and locally for the AWS-shaped test tier.

## 18. Definition of Done (per change, per guidelines)

A change is "done" when:

- Behavior is exercised by a test (unit, integration, or e2e), including negative-space tests for every new validation path.
- Every new/touched function has at least two meaningful assertions.
- Every new bound is a named const in `kleya-core/src/limits.rs`.
- `cargo fmt`, `cargo clippy --all-targets --all-features -D warnings`, `cargo nextest run` pass locally.
- Commit description states the why.
- PR description lists architecture-level changes (ports, adapters, surfaces).
