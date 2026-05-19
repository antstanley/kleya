# 09 — Architecture Principles

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya` is a four-crate Rust workspace shaped as a hexagonal (ports / adapters) application: domain logic and traits live in `kleya-core`; AWS-specific I/O lives in `kleya-aws`; the CLI binary that wires them together lives in `kleya-cli`; embedded assets live in `kleya-bootstrap-assets`. The rule is one-way dependency flow inward toward `kleya-core`; no provider SDK ever crosses the port boundary.

---

## Workspace layout

```
kleya/
├── Cargo.toml                       # workspace manifest + dependency catalog
├── rust-toolchain.toml              # pinned 1.95.0
├── clippy.toml                      # pedantic-adjacent lints
├── deny.toml                        # cargo-deny config (licenses + advisories)
├── lefthook.yml                     # pre-push hook
├── crates/
│   ├── kleya-core/                  # domain + ports; no I/O, no provider SDKs
│   │   ├── src/error.rs
│   │   ├── src/limits.rs
│   │   ├── src/config.rs
│   │   ├── src/model/               # InstanceId, KeyName, Region, TemplateSpec, …
│   │   ├── src/ports/               # CloudCompute, KeyStore, Clock, IdGen
│   │   ├── src/commands/            # LaunchService, ConnectService, …
│   │   ├── src/bootstrap/           # pure render + encode for user-data
│   │   ├── src/test_support/        # InMemoryCompute, InMemoryKeyStore (feature-gated)
│   │   └── src/util.rs              # wait_or_cancel — shared async helper
│   ├── kleya-aws/                   # CloudCompute impl backed by aws-sdk-ec2
│   │   ├── src/client.rs            # build_ec2_client(region, endpoint_url)
│   │   ├── src/ec2.rs               # CloudCompute impl
│   │   ├── src/mapping.rs           # SDK ↔ core type mapping
│   │   └── tests/floci/             # Floci-backed integration tests
│   ├── kleya-cli/                   # the `kleya` binary
│   │   ├── src/main.rs              # ~30 lines: parse → SIGINT handler → dispatch
│   │   ├── src/clap_args.rs         # clap derive types only
│   │   ├── src/config_loader.rs     # multi-format loader
│   │   ├── src/dispatch.rs          # match Cmd { ... } orchestration
│   │   ├── src/ssh_probe.rs         # probe_ssh_ready + wait_cloud_init
│   │   ├── src/key_store_fs.rs      # filesystem KeyStore impl
│   │   ├── src/exit_code.rs         # Error → i32
│   │   └── src/logging.rs           # tracing-subscriber setup
│   └── kleya-bootstrap-assets/      # embeds setup_devbox.sh.j2 + ghostty.terminfo
│       ├── src/lib.rs
│       └── assets/
│           ├── setup_devbox.sh.j2
│           └── ghostty.terminfo
├── docs/
│   ├── specs/                       # this directory — canonical specs
│   └── superpowers/specs/           # historical draft design docs (kept for context)
├── .github/workflows/
│   ├── ci.yml                       # fmt, clippy, nextest, deny, llvm-cov, floci, skill-package
│   └── release.yml                  # cargo-dist; tag-triggered
├── scripts/
│   ├── package_skill.py             # builds the .skill archive
│   └── install-skill.sh             # installs the skill across detected agents
├── skills/
│   └── using-kleya/                 # SKILL.md + supporting docs
├── ONBOARDING.md
├── README.md
└── CONTRIBUTING.md
```

The workspace is the unit of release — `cargo-dist` builds one `kleya-cli` binary per target triple and a single installer.

---

## Dependency graph

```
                 ┌────────────────────────────────────────┐
                 │             kleya-cli (bin)            │
                 │  → kleya-core                          │
                 │  → kleya-aws                           │
                 │  → kleya-bootstrap-assets              │
                 │  → clap, tokio, tracing, shellexpand,  │
                 │    ssh-key, md-5, hex, …               │
                 └─────────────┬──────────────────────────┘
                               │
        ┌──────────────────────┼────────────────────────┐
        ▼                      ▼                        ▼
┌────────────────────┐ ┌────────────────────┐ ┌────────────────────────────┐
│      kleya-aws     │ │      kleya-core    │ │  kleya-bootstrap-assets    │
│  → kleya-core      │ │ (no provider SDKs) │ │ (no kleya-* deps)          │
│  → aws-sdk-ec2     │ │  → serde, thiserror│ │  include_str! only         │
│  → aws-sdk-ssm     │ │  → once_cell, regex│ │                            │
│  → tokio, …        │ │  → minijinja, base64│ │                            │
└─────────┬──────────┘ │  → flate2, …       │ └────────────────────────────┘
          │            └────────────────────┘
          ▼
   aws-config / aws-sdk-*
```

Rules:

1. `kleya-core` is **leaf-ward** — it depends on no other crate in the workspace. It depends on `kleya-bootstrap-assets` for the embedded constants (one-way; bootstrap-assets has no kleya deps).
2. `kleya-aws` depends on `kleya-core` only. It must never reference `kleya-cli`. The `client::build_ec2_client(region, endpoint_url)` signature is shaped to support the Floci emulator override without `kleya-core` knowing.
3. `kleya-cli` is the only crate allowed to build adapter instances and inject them into core services. Tests live throughout but the wiring is here.
4. `kleya-bootstrap-assets` has zero workspace dependencies — it's a thin `include_str!` crate. Keeping it isolated means rebuilds when the asset changes only affect the binaries that re-link.

A clippy lint or build assertion catching `aws_sdk_*` references in `kleya-core` is out of scope; the rule is enforced by review.

---

## Hexagonal layering

```
                 ┌──────────────────────────────────────────────┐
                 │                  Drivers                     │
                 │  clap_args ─▶ dispatch ─▶ ConnectService …   │
                 │              (in kleya-cli)                  │
                 └──────────────────────┬───────────────────────┘
                                        │ injects adapters via Arc<dyn …>
                                        ▼
                 ┌──────────────────────────────────────────────┐
                 │                  Domain                      │
                 │  commands/ ─ model/ ─ bootstrap/ ─ config/   │
                 │  ports/ define trait CloudCompute, KeyStore  │
                 │              (in kleya-core)                 │
                 └──────────────────────┬───────────────────────┘
                                        │ impl CloudCompute for AwsEc2
                                        ▼
                 ┌──────────────────────────────────────────────┐
                 │                Adapters                      │
                 │  AwsEc2 ─ aws-sdk-ec2 + aws-sdk-ssm          │
                 │  FsKeyStore ─ std::fs + ssh-key + md-5       │
                 │       (in kleya-aws, kleya-cli)              │
                 └──────────────────────────────────────────────┘
```

- **Drivers** (top): the CLI parses input, builds adapter instances, and hands them to domain services. Nothing else above the domain has business logic.
- **Domain** (middle): pure orchestration on top of port traits. No `tokio::fs`, no `std::process::Command`, no `aws_sdk_*`. The one exception is `tokio::time` via `wait_or_cancel`, which is provider-neutral.
- **Adapters** (bottom): own all I/O. `AwsEc2` for the EC2 + SSM side, `FsKeyStore` for the filesystem side. Errors translate at the adapter boundary into `kleya_core::Error::Adapter`.

The split is testable: command-level integration tests in `kleya-core/tests/` exercise the domain against `InMemoryCompute` + `InMemoryKeyStore` with no AWS dependency.

---

## Runtime model

- **Async runtime.** `#[tokio::main(flavor = "current_thread")]` — the CLI does not need the multi-thread scheduler. One executor, one current-thread runtime, all work driven by `await` against `tokio::time` / `tokio::net` / `tokio::process`.
- **Cancellation.** Single `tokio_util::sync::CancellationToken` constructed in `main.rs`, cloned into the SIGINT handler and into every poll loop. `kleya_core::util::wait_or_cancel(interval, cancel)` is the only place the token is observed; callers see a `true` return as cancellation and short-circuit with `Error::Cancelled { instance }`.
- **No recursion.** All loops have an explicit `attempt_count < MAX` bound. `instance_wait_running` derives `max_attempts = timeout / poll_interval + 2`; `probe_ssh_ready` uses `elapsed >= timeout`.
- **Process replacement on `connect`.** The CLI's `Cmd::Connect` arm and the `--connect` post-launch path use `std::os::unix::process::CommandExt::exec()` to replace kleya with ssh. Ctrl-C from then on goes to ssh, not kleya; ssh's exit code becomes kleya's exit code. This is the source of the Unix-only restriction.

---

## Module-level conventions in `kleya-core`

| Module | Role |
|---|---|
| `error` | One `Error` enum; `Result<T, E = Error>` alias; `adapter(provider, source)` constructor |
| `limits` | All named bounds; compile-time `const _: () = assert!(…)` cross-checks |
| `config` | `serde(deny_unknown_fields)` structs + `Config::validate()` |
| `model/` | Newtypes with constructor validation; `Lazy<Regex>` for static patterns |
| `ports/` | Trait definitions only — no `impl` blocks beyond default constructors for sentinel impls (`SystemClock`, `AdjAnimalIdGen`) |
| `commands/` | One file per subcommand; orchestration sits on the ports |
| `bootstrap/` | Pure `render` + `encode_user_data*`; no I/O |
| `test_support/` | Feature-gated `InMemoryCompute`, `InMemoryKeyStore`, `FakeClock`, `FakeIdGen` |
| `util` | Shared async helpers (`wait_or_cancel`); nothing provider-specific |

No file in `kleya-core/src/` references `aws_sdk_*` or `tokio::fs`. The single file-IO path in `kleya-core` is the override-script read in `LaunchService::render_user_data`, which uses `tokio::fs::read` — the I/O lives in core because it's part of the launch orchestration but does not need a port (it reads a single operator-supplied path).

---

## Module-level conventions in `kleya-aws`

| Module | Role |
|---|---|
| `client` | `build_ec2_client(region, endpoint_url)` — encapsulates `aws-config` and the Floci endpoint override |
| `ec2` | `AwsEc2` struct + `CloudCompute` impl + `build_request_launch_template_data` |
| `mapping` | `map_instance(&aws_sdk_ec2::types::Instance) -> Option<Instance>` |
| `error` | `AwsError` enum + `From<AwsError> for kleya_core::Error` |
| `tests/floci/` | Floci-backed integration tests gated by `KLEYA_TEST_FLOCI=1` |

The adapter exposes the `AwsEc2` struct directly because there is exactly one concrete implementation per crate. A future GCP adapter would expose `GcpCompute` analogously; `kleya-cli/src/dispatch.rs` chooses which to instantiate based on configuration.

---

## Module-level conventions in `kleya-cli`

| Module | Role |
|---|---|
| `main.rs` | Argv parse, tracing init, SIGINT handler, dispatch entry, exit-code translation. ~30 lines. |
| `clap_args.rs` | `clap::Parser` / `clap::Subcommand` derive types only — no logic |
| `config_loader.rs` | Multi-format config load + path resolution |
| `dispatch.rs` | `run(cli, cancel)` → `match cli.command { ... }`; builds services and adapters |
| `ssh_probe.rs` | TCP-22 probe and `cloud-init status --wait` runner |
| `key_store_fs.rs` | `FsKeyStore: KeyStore` impl |
| `exit_code.rs` | `code_for(&Error) -> i32` |
| `logging.rs` | `tracing-subscriber` init (text vs. JSON layer) |
| `tests/` | `cli_smoke.rs`, `config_roundtrip.rs`, `key_store_fs.rs` |

`main.rs` stays tiny on purpose — everything that has a unit test lives in `dispatch.rs` or below, so `main` is the glue you can read at a glance.

---

## Compile-time invariants

In `kleya-core/src/limits.rs`:

```rust
const _: () = assert!(LAUNCH_POLL_INTERVAL_SECONDS <= LAUNCH_WAIT_SECONDS_MAX);
const _: () = assert!(SSH_PROBE_INTERVAL_SECONDS <= SSH_PROBE_TIMEOUT_SECONDS);
const _: () = assert!(AWS_RETRY_BACKOFF_BASE_MS <= AWS_RETRY_BACKOFF_CAP_MS);
const _: () = assert!(USER_DATA_GZIP_BYTES_MAX <= USER_DATA_RAW_BYTES_MAX);
const _: () = assert!(USER_DATA_BASE64_BYTES_MAX >= USER_DATA_RAW_BYTES_MAX);
const _: () = assert!(TAG_KEY_BYTES_MAX > 0 && TAG_VALUE_BYTES_MAX > 0);
```

Anyone tweaking a limit that breaks these relationships sees a compile error, not a runtime surprise.

---

## Logging shape

[crates/kleya-cli/src/logging.rs](../../crates/kleya-cli/src/logging.rs):

- `tracing` + `tracing-subscriber` with an `EnvFilter` derived from the verbosity flag.
- Default human-readable layer; `--log-format json` swaps to a JSON layer. Mandatory structured fields where applicable: `command`, `region`, `template`, `instance_id`.
- Secrets never logged. The SSH argv (which contains the key path) is logged at `debug` only. `KLEYA_DEBUG_SECRETS=1` is the documented opt-in for full-argv dumps; it is honoured only when `-v` is also at `debug` or higher.
- Named-limit hits emit `tracing::warn!` with structured `limit_hit` fields so operators can grep / alert on them.

---

## Assumptions and open questions

**Assumptions**

- Unix-only platforms (Linux + macOS). The `execvp` based `connect` path makes this load-bearing; Windows is documented as out of scope.
- A current-thread tokio runtime is sufficient. No part of the CLI needs CPU parallelism — every blocking moment is I/O against AWS.
- The four crates fit naturally on the dependency graph above. A future provider crate (`kleya-gcp`, `kleya-hetzner`) follows the same shape as `kleya-aws` — depend on `kleya-core` only, expose one struct that implements `CloudCompute`.

**Decisions**

- *Hexagonal layering enforced by crate boundaries, not just module names.* **The compiler refuses to import `aws_sdk_ec2` from `kleya-core` because the dependency isn't in `kleya-core/Cargo.toml`.** This is a stronger guarantee than a clippy lint.
- *`kleya-cli` is the only DI site.* **All `Arc<dyn CloudCompute>` and `Arc<dyn KeyStore>` constructions happen in `dispatch::run` / `dispatch::run_with`.** Tests use `run_with` to inject in-memory fakes; production uses `run` which builds the AWS-backed adapters.
- *`kleya-bootstrap-assets` is its own crate, not a module of `kleya-core`.* **Asset changes don't force `kleya-core` recompiles.** A change to the bootstrap script re-links the binary but leaves `kleya-core`'s `target/` cache intact.
- *`#[tokio::main(flavor = "current_thread")]`.* **One executor, no implicit Send bounds.** Multi-thread is overkill for a CLI that issues one AWS call at a time.
- *Process replacement (`execvp`) on `connect`.* **Cleaner Ctrl-C, ssh exit code passes through.** The alternative — spawn and wait — leaves kleya as ssh's parent and complicates signal handling.

**Open questions**

- *Provider selection at the CLI layer.* Resolved: the provider is chosen via a `provider` config field, with a `--provider` CLI flag override. `dispatch::run` reads that selection rather than constructing `AwsEc2` unconditionally.
- *Common adapter test harness.* Resolved: each adapter ships its own test harness (Floci for AWS, equivalents for future providers). Do not factor out a shared harness.
