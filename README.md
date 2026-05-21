# kleya

A small Rust CLI that bootstraps short-lived cloud development boxes for agentic coding sessions. Zero-config by default — `kleya launch` provisions a spot / preemptible instance, runs an embedded bootstrap that installs the usual agent toolchain (zsh, oh-my-zsh, tmux, git, rust, node, jj, python, uv, Claude Code), and prints an `ssh` invocation to attach.

> **Status:** v0.1.0-rc.3 prerelease. Unix only (Linux + macOS, x86_64 + aarch64). Windows is out of scope.

**Provider support.** kleya is built around a provider-neutral `CloudCompute` port in `kleya-core`. Cloud-specific code lives in adapter crates (`kleya-aws`, and any future siblings) that depend only on `kleya-core` — adding a new provider is a new crate, not a refactor of the binary. **The only adapter shipped in v0.1 is AWS EC2** (Amazon Linux 2023, ARM or x86); the design and tradeoffs of the port are documented in [`docs/specs/04-provider-port.md`](docs/specs/04-provider-port.md). The rest of this README assumes the AWS adapter; sections that are adapter-specific are marked accordingly.

## Prerequisites

**Always required:**

- The `ssh` binary on your `PATH` (kleya `exec`s it for `connect`).
- `tmux` on the **remote** instance (installed by the bootstrap script).

**AWS adapter (the only provider in v0.1):**

- An AWS account with:
  - Working credentials reachable through the SDK default chain (env vars, profile, IAM Identity Center cached token, `aws login` console credentials, `credential_process` helper, web-identity OIDC, or IMDS — see [`docs/specs/11-credentials-and-sso.md`](docs/specs/11-credentials-and-sso.md)). Authentication happens outside kleya (`aws sso login`, `aws login`, `aws configure`, etc.).
  - A **default VPC** in the region you launch into.
  - Permission to call `ec2:*` for templates, instances, security groups, key pairs, and `ssm:GetParameter` for the AL2023 AMI alias.

## Install

### One-liner (recommended)

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/antstanley/kleya/releases/download/v0.1.0-rc.3/kleya-cli-installer.sh | sh
```

Installs `kleya` to `~/.cargo/bin/` (or `~/.local/bin/`, whichever is on your PATH).

### Manual download

Grab the right tarball from [the releases page](https://github.com/antstanley/kleya/releases) for your target:

- `kleya-cli-x86_64-unknown-linux-gnu.tar.xz`
- `kleya-cli-aarch64-unknown-linux-gnu.tar.xz`
- `kleya-cli-x86_64-apple-darwin.tar.xz`
- `kleya-cli-aarch64-apple-darwin.tar.xz`

Each tarball contains a single `kleya` binary. A matching `.sha256` checksum file is published alongside each tarball; `sha256.sum` aggregates all of them.

### From source

```bash
git clone https://github.com/antstanley/kleya.git
cd kleya
cargo install --path crates/kleya-cli --locked
```

Requires the workspace toolchain (Rust 1.95.0; `rust-toolchain.toml` pins it).

## Quickstart

```bash
# 1. Launch a zero-config spot instance and wait for it to be reachable
kleya launch --connect

# 2. Once you're in, work happens on the remote box. Detach with Ctrl-b d.

# 3. Re-attach from another terminal
kleya connect <name>     # the name printed at launch

# 4. List your kleya-managed instances
kleya list

# 5. Tear it down
kleya terminate <name>
```

`kleya launch --connect` waits for SSH to come up, waits for cloud-init to finish (so you land on a fully bootstrapped box), and then `exec`s ssh + `tmux new-session -A`. The named tmux session means subsequent `kleya connect <name>` calls reattach to the same session.

## Commands

The CLI surface is provider-neutral by intent — every subcommand maps onto a method on `CloudCompute`. The concrete defaults (`m8g.xlarge` instance type, `amazon-linux-2023-arm64` AMI alias, etc.) are AWS-shaped because AWS is the only adapter in v0.1; an additional provider would resolve them from its own catalog.

### `kleya launch`

Launches a spot (default) instance from a template.

```
kleya launch [--template <name>] [--name <name>]
             [--instance-type <type>] [--market spot|on-demand]
             [--connect] [--wait-bootstrap] [--no-wait-bootstrap]
             [--dry-run]
```

| Flag | Meaning |
|---|---|
| `--template <name>` | Use a template defined in your config (default: an auto-created `default` template) |
| `--name <name>` | Tag the instance with this human-readable name. Auto-generated if omitted (`kleya-<adj>-<animal>`) |
| `--instance-type <t>` | Override the template's instance type (e.g. `m7g.xlarge`, `t4g.medium`) |
| `--market spot\|on-demand` | Override the template's market |
| `--connect` | After SSH is reachable, wait for cloud-init and `exec` ssh. Implies `--wait-bootstrap` unless overridden |
| `--wait-bootstrap` | Run `cloud-init status --wait` over SSH before returning |
| `--no-wait-bootstrap` | Opt out of the cloud-init wait when `--connect` is set (TCP-22 only) |
| `--dry-run` | Resolve and print the launch plan; exit 0 without provisioning |

### `kleya list`

Lists instances tagged `kleya:managed=true`.

```
kleya list [--json]
```

### `kleya connect`

Resolves a handle (name or a provider-native instance id, e.g. `i-…` for AWS) to a managed instance, looks up the right private key via the `kleya:key` tag, and `exec`s ssh.

```
kleya connect <name> [--print] [--no-tmux] [--tmux-session <s>] [--instance-id <id>]
```

| Flag | Meaning |
|---|---|
| `--print` | Print the ssh argv and exit; don't actually connect |
| `--no-tmux` | Skip the `tmux new-session -A` wrapper |
| `--tmux-session <s>` | Override the configured tmux session name |
| `--instance-id <id>` | Resolve by the provider's native instance id instead of name. AWS adapter accepts `i-…`. Useful for unmanaged instances. |

### `kleya terminate`

```
kleya terminate <name> [--yes]
```

Confirms interactively unless `--yes` is passed.

### `kleya template`

Templates capture provider-specific launch configuration so `kleya launch --template <n>` is a one-shot. On the AWS adapter these map to EC2 Launch Templates.

```
kleya template create --name <n> [--ami ami-...] [--instance-type <t>] [--key-name <k>] [--user-data <path>]
kleya template update --name <n> [...same flags...]
kleya template list [--json]
kleya template delete <name> [--yes]
```

The `--ami` flag is AWS-adapter-specific; other adapters expose their own equivalent (machine image, machine type, project / zone, …).

### `kleya config`

```
kleya config show       # print the merged config (defaults + file + flags) as TOML
kleya config path       # print the path of the file that was actually loaded (or "<defaults>")
```

## Agent skill (optional)

If you drive kleya from a coding agent (Claude Code, Cursor, OpenCode, Codex), you can install a companion **skill** that teaches the agent the launch / connect / templates / unattended-handoff workflows so it stops reaching for raw `aws ec2` commands.

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/antstanley/kleya/releases/download/v0.1.0-rc.3/install-skill.sh | sh
```

The installer autodetects which agents you have configured (it looks for `~/.claude/`, `~/.cursor/`, `~/.config/opencode/`, `~/.agents/`, `~/.codex/`) and writes the skill to each one's native location. Override with `--target=claude,opencode` (comma-separated) or `--target=all`.

- `~/.claude/skills/using-kleya/SKILL.md` — Claude Code
- `~/.cursor/skills/using-kleya/SKILL.md` — Cursor (also picks up the Claude path)
- `~/.config/opencode/skills/using-kleya/SKILL.md` — OpenCode
- `~/.agents/skills/using-kleya/SKILL.md` — `.agents` folder spec (cross-agent fallback)
- `~/.codex/AGENTS.md` — Codex (appended between idempotent marker comments)

Restart your agent after install. To pin a specific version, pass `--version=v0.1.0-rc.3`.

## Configuration

Optional. `kleya launch` with no flags and no config file launches a working dev box.

### File location

Resolved in this order:

1. `--config <path>` flag
2. `KLEYA_CONFIG` environment variable
3. `~/.config/kleya/config.toml` (or `.yaml`, `.json`, `.jsonc`)

### Format

TOML, YAML, JSON, and JSONC are all accepted. Format is detected from the extension.

### Example (`~/.config/kleya/config.toml`)

Field names marked _(AWS adapter)_ are interpreted by the AWS adapter; other adapters will accept their own equivalents.

```toml
default_region  = "eu-west-1"    # AWS region for the AWS adapter
default_profile = "default"      # AWS named profile (AWS adapter)

[defaults]
instance_type = "m8g.xlarge"
market        = "spot"           # or "on-demand"
spot_type     = "one-time"       # or "persistent"
ami_alias     = "amazon-linux-2023-arm64"   # AWS adapter — SSM-resolved at launch

[bootstrap]
# Optional path to a custom user-data script. If set, install_ghostty_terminfo
# has no effect (the script is passed through verbatim, base64-encoded).
# user_data_path = "~/.config/kleya/bootstrap.sh"
install_ghostty_terminfo = true

[ssh]
user         = "ec2-user"
tmux         = true
tmux_session = "kleya"
term         = "xterm-256color"  # TERM sent to the remote pty; "" sends local $TERM
extra_args   = []            # appended verbatim to the ssh argv

[keys]
dir              = "~/.config/kleya/keys"
default_key_name = "kleya-default"

# Optional per-template overrides. Templates are created on demand by
# `kleya template create`, or implicitly by `kleya launch --template <name>`.
[[templates]]
name           = "gpu"
instance_type  = "g6.xlarge"
# ami_id           = "ami-..."           # optional; otherwise resolved from ami_alias
# key_name         = "..."
# security_group_ids = ["sg-..."]
# subnet_id        = "subnet-..."

[[templates.tags]]
key   = "Project"
value = "gpu-experiments"
```

## Environment variables

| Variable | Effect |
|---|---|
| `KLEYA_CONFIG` | Path to config file (overrides default search) |
| `KLEYA_PROFILE` | Provider profile to use (overrides `default_profile`). On the AWS adapter, this is the AWS named profile. |
| `KLEYA_REGION` | Provider region (overrides `default_region`). AWS adapter passes this to the SDK. |
| `KLEYA_LOG_FORMAT` | `text` (default) or `json` |
| `AWS_*` | Standard AWS SDK env vars (`AWS_ACCESS_KEY_ID`, `AWS_PROFILE`, …) — honoured by the SDK itself; kleya does not read them directly. |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | User-data exceeds limits |
| 2 | Config invalid / pre-flight check failed |
| 3 | Instance not found |
| 4 | Handle ambiguous (multiple instances match the name) |
| 5 | SSH not ready within timeout (default 180s) |
| 6 | Launch wait timed out (default 600s) |
| 7 | Key mismatch (local fingerprint differs from the provider's record) or key orphaned (provider has the key, local pem missing) |
| 70 | Provider adapter error (SDK / network / API). On the AWS adapter this includes anything `aws-sdk-ec2` / `aws-sdk-ssm` surfaces. |
| 74 | I/O error |
| 130 | Cancelled (Ctrl-C / SIGINT) |

## Troubleshooting

Most adapter-specific failures surface as `Error::Adapter { provider, source }` (exit code 70) with the provider's own error in the `source` field. The list below is for AWS-adapter-specific cases that have a known kleya-side remediation.

**`Adapter aws-ec2: ... no default VPC` (AWS adapter).** kleya's zero-config path assumes a default VPC in the chosen region. Either create one (`aws ec2 create-default-vpc`) or specify `subnet_id` and `security_group_ids` in a `[[templates]]` block.

**`KeyMismatch`.** Your local `~/.config/kleya/keys/<name>.pem` fingerprint differs from what the provider has registered. Either delete the local pem (kleya will treat as orphaned and you can re-import the provider's public half manually), or remove the provider-side key and let kleya regenerate on next launch (`kleya launch --regenerate-key`).

**`SshNotReady` after 180s.** The probe couldn't reach port 22. Check the provider's firewall / security group allows your IP and that the instance actually started (`kleya list` or the provider console).

**`launch --connect` drops you mid-bootstrap.** Use `--wait-bootstrap` (the default when `--connect` is set unless `--no-wait-bootstrap` is also passed). The wait runs `cloud-init status --wait` over SSH before returning control.

## Design

The full design lives in [`docs/specs/`](docs/specs/) — eleven numbered pages indexed by [`docs/README.md`](docs/README.md). Start with [`docs/specs/00-overview.md`](docs/specs/00-overview.md). The pages most relevant to extending kleya:

- [`04-provider-port.md`](docs/specs/04-provider-port.md) — the `CloudCompute` trait, idempotency contract, and what a new adapter has to implement.
- [`06-launch-and-connect.md`](docs/specs/06-launch-and-connect.md) — launch orchestration, key lifecycle, SSH probe.
- [`11-credentials-and-sso.md`](docs/specs/11-credentials-and-sso.md) — credentials chain, profile / region resolution, and why kleya never owns login.

## License

Dual-licensed under `Apache-2.0 OR MIT` per [`Cargo.toml`](Cargo.toml#L13). You may use this crate under the terms of either, at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for repo layout, tooling, conventions, and the release process. The canonical design spec at [`docs/specs/`](docs/specs/) is the source of truth for non-trivial changes.
