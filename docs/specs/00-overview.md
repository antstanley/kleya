# kleya — Design Overview

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley · **Scope:** Repo-wide

`kleya` is a small Rust CLI that bootstraps AWS EC2 spot instances as Claude-Code-ready development boxes. Zero-config by default: `kleya launch` with no flags and no config file provisions an Amazon Linux 2023 ARM box, installs the agent toolchain (zsh, tmux, git, rust, node, jj, python, uv, Claude Code), and prints an `ssh` invocation to attach.

This document is the entry point. Detail pages are linked from each section.

---

## Problem

Operators who drive agentic coding sessions on disposable cloud boxes need a reproducible way to launch, attach to, and tear down a development environment. The legacy approach was a shell pipeline (`launch.sh`) wrapping `aws ec2 …` calls — untyped, hard to test, AWS-only, and brittle to refactor. `kleya` replaces it with a typed, testable, port/adapter-shaped Rust CLI so:

- Bootstrapping is deterministic (embedded user-data, named limits, snapshot-tested rendering).
- The cloud provider is a swappable port — `kleya-core` does not depend on `aws-sdk-ec2`.
- Failures map to typed errors and stable exit codes that operators (and agents) can script against.

---

## Goals

1. Launch a working dev box with zero configuration — `kleya launch` resolves every option to a built-in default.
2. Reach a Claude-Code-ready remote (zsh, tmux, rust, node, jj, python, uv, Claude Code, server-side `xterm-ghostty` terminfo) via a single embedded user-data script.
3. Idempotent default-resource lifecycle — security group, keypair, launch template are created on first run and reused on subsequent runs.
4. Provider port (`CloudCompute`) in `kleya-core`; the EC2 implementation lives in `kleya-aws`. Adding a non-AWS backend is a new crate.
5. Tiger-style discipline — no `unwrap`/`expect`/`panic!` in production paths, all bounds are named constants, every function has at least two assertions.
6. CLI flags, environment variables, and config files (TOML / YAML / JSON / JSONC) feed the same validated `Config`.
7. Stable exit codes mapping each `Error` variant to a documented integer.
8. Cancellable via SIGINT — Ctrl-C surfaces `Error::Cancelled` with exit code 130.

## Non-goals

- Windows support. `kleya` calls `execvp` and Unix mode bits; Windows is documented as out of scope.
- Multi-region orchestration in a single command — one region per invocation.
- Spot interruption recovery / re-launch.
- Non-AWS provider implementations (the port exists; implementations come in later specs).
- Cost reporting.
- China and GovCloud regions — the `Region` regex `^[a-z]{2}-[a-z]+-[0-9]+$` accepts only the commercial form.
- Automatic cleanup of pre-existing launch templates not managed by `kleya`.

---

## System shape

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              kleya-cli (bin)                             │
│  clap_args ─ config_loader ─ logging ─ dispatch ─ ssh_probe ─ FsKeyStore │
└──────────┬─────────────────────────────────────────────┬─────────────────┘
           │ Arc<dyn CloudCompute>      Arc<dyn KeyStore>│
           ▼                                             ▼
┌────────────────────────────────┐       ┌────────────────────────────────┐
│           kleya-aws            │       │          kleya-core            │
│  AwsEc2: impl CloudCompute     │       │  ports/        commands/       │
│  — aws-sdk-ec2 + aws-sdk-ssm   │       │  model/        bootstrap/      │
│  — error mapping → adapter()   │       │  config/       limits/  util/  │
└────────────────────────────────┘       │  test_support/ (feature-gated) │
                                         └────────────────┬───────────────┘
                                                          │ include_str!
                                                          ▼
                                       ┌────────────────────────────────┐
                                       │     kleya-bootstrap-assets     │
                                       │  setup_devbox.sh.j2            │
                                       │  ghostty.terminfo              │
                                       └────────────────────────────────┘
```

- **`kleya-cli`** parses argv, loads config, builds adapter instances, and dispatches subcommand orchestration.
- **`kleya-core`** holds all domain types, ports, and orchestration. I/O-free and provider-SDK-free.
- **`kleya-aws`** implements `CloudCompute` against the AWS SDK. Errors wrap into `kleya_core::Error::Adapter`.
- **`kleya-bootstrap-assets`** embeds the user-data template and `ghostty.terminfo` source via `include_str!`.

---

## Detail pages

| Page | Topic |
|---|---|
| [01-domain-model.md](01-domain-model.md) | Newtypes, regexes, entity tables, lifecycles for instance / template / key |
| [02-cli-surface.md](02-cli-surface.md) | Commands, flags, environment variables, exit codes |
| [03-configuration.md](03-configuration.md) | Multi-format loader, precedence, canonical schema, validation, round-trip |
| [04-provider-port.md](04-provider-port.md) | `CloudCompute` and `KeyStore` traits; idempotency contract; AWS adapter mapping |
| [05-bootstrap-rendering.md](05-bootstrap-rendering.md) | User-data template, ghostty terminfo, size budgets, override semantics |
| [06-launch-and-connect.md](06-launch-and-connect.md) | Launch orchestration, key lifecycle, connect flow, SSH probe, cloud-init wait, cancellation |
| [07-error-model.md](07-error-model.md) | `Error` enum, exit-code mapping, adapter boundary |
| [08-testing.md](08-testing.md) | Test tiers, Floci, snapshots, property tests, coverage floor |
| [09-architecture-principles.md](09-architecture-principles.md) | Crate layout, hexagonal layering, dependency graph, runtime/process model |
| [10-development-guidelines.md](10-development-guidelines.md) | Tiger-style rules, named limits, hooks, CI, release |
| [11-credentials-and-sso.md](11-credentials-and-sso.md) | AWS credentials chain, profile / region resolution, SSO via cached tokens, why kleya never owns login |
| [canonical-types.schema.json](canonical-types.schema.json) | JSON Schema for every domain entity referenced above |

---

## Scope summary

| Area | Implementation | Notes |
|---|---|---|
| Cloud provider | AWS EC2 via `aws-sdk-ec2` + `aws-sdk-ssm` | `CloudCompute` port is provider-neutral; no other adapter exists in this branch |
| Platforms | Unix (Linux + macOS), x86_64 + aarch64 | `execvp`-based connect; Windows out of scope |
| Configuration | TOML / YAML / JSON / JSONC | Same `Config` struct via serde; validated identically across formats |
| Bootstrap | Embedded `setup_devbox.sh.j2` rendered via `minijinja` | Ghostty terminfo installed server-side via `tic` |
| Key storage | `FsKeyStore` at `~/.config/kleya/keys` (0700; pem 0600) | Ed25519, OpenSSH format, EC2-style MD5-of-DER-SPKI fingerprints |
| Concurrency | `tokio` current-thread runtime | SIGINT propagates via `CancellationToken` → `Error::Cancelled` (exit 130) |
| Telemetry | `tracing` + `tracing-subscriber` | Default text; `--log-format json` switches layers |
| Release | `cargo-dist`, tag-triggered | Targets: `{x86_64,aarch64}-{unknown-linux-gnu,apple-darwin}` |

---

## Assumptions and open questions

**Assumptions**

- The operator has working AWS credentials reachable through `aws-config`'s default chain (env vars, profile, SSO cached token, `credential_process`, web-identity OIDC, or IMDS). Authentication itself happens outside kleya — `aws sso login`, `aws configure`, `aws-vault`, `granted`, etc. See [11-credentials-and-sso.md](11-credentials-and-sso.md).
- A default VPC exists in the chosen region. Absence surfaces as `Error::ConfigInvalid { reason: "no default VPC in region X" }`.
- The user's Ghostty client recognises `xterm-ghostty` locally; this design only installs the terminfo server-side.
- Docker is available for running Floci in CI when the AWS-shaped test tier is enabled.
- The operator's local clock is approximately correct (AWS SDK request signing depends on it).

**Decisions**

- *Single-tool repo, single spec layer.* **All canonical specs live under `docs/specs/` with no per-app layer.** `kleya` is one CLI binary built from a four-crate workspace; the global / per-app split would be ceremony without a second app to anchor it.
- *Status field uses "Implemented", not "Draft".* **The current canonical spec describes v0.1.0-rc.2 as shipped.** The legacy design docs at `docs/superpowers/specs/` remain as historical drafts; this set is the authoritative reference for what is checked in.
- *Provider-neutral core.* **`kleya-core` never depends on `aws-sdk-*`.** This makes adding a non-AWS adapter a new sibling crate rather than a `kleya-core` refactor; see [09-architecture-principles.md](09-architecture-principles.md).

**Open questions**

- *Non-AWS adapter shape.* When a second provider (GCP / Hetzner / bare-metal) is added, will `CloudCompute` need extending for provider-specific notions like preemption notifications, or can it remain as-is? Resolved when the second adapter lands.
- *China / GovCloud regions.* `Region` validation refuses `cn-*` and `us-gov-*`. If demand emerges, the regex lifts in a follow-up spec rather than ad-hoc here.
- *Security group hardening.* The auto-created `kleya-default` SG opens `22/tcp` from `0.0.0.0/0`. Tightening (e.g., to the launching operator's egress IP) is left to a later spec; see [06-launch-and-connect.md](06-launch-and-connect.md).
