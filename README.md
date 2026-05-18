# kleya

A small Rust CLI that bootstraps AWS EC2 spot instances as Claude-Code-ready development boxes. Zero-config by default — `kleya launch` provisions an Amazon Linux 2023 ARM instance, installs zsh / oh-my-zsh / tmux / git / rust / node / jj / python / uv / Claude Code, and prints an `ssh` invocation to attach.

> **Status:** v0.1.0-rc.2 prerelease. Unix only (Linux + macOS, x86_64 + aarch64). Windows is out of scope.

## Prerequisites

- An AWS account with:
  - Programmatic credentials (env vars, profile, or instance role)
  - A **default VPC** in the region you launch into
  - Permission to call `ec2:*` for templates, instances, security groups, key pairs, and `ssm:GetParameter` for the AL2023 AMI alias
- The `ssh` binary on your `PATH` (kleya `exec`s it for `connect`)
- `tmux` on the **remote** instance (installed by the bootstrap script)

## Install

### One-liner (recommended)

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/antstanley/kleya/releases/download/v0.1.0-rc.2/kleya-cli-installer.sh | sh
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

Resolves a handle (name or `i-...`) to a managed instance, looks up the right private key via the `kleya:key` tag, and `exec`s ssh.

```
kleya connect <name> [--print] [--no-tmux] [--tmux-session <s>] [--instance-id i-...]
```

| Flag | Meaning |
|---|---|
| `--print` | Print the ssh argv and exit; don't actually connect |
| `--no-tmux` | Skip the `tmux new-session -A` wrapper |
| `--tmux-session <s>` | Override the configured tmux session name |
| `--instance-id i-...` | Resolve by AWS instance id instead of name (useful for unmanaged instances) |

### `kleya terminate`

```
kleya terminate <name> [--yes]
```

Confirms interactively unless `--yes` is passed.

### `kleya template`

```
kleya template create --name <n> [--ami ami-...] [--instance-type <t>] [--key-name <k>] [--user-data <path>]
kleya template update --name <n> [...same flags...]
kleya template list [--json]
kleya template delete <name> [--yes]
```

### `kleya config`

```
kleya config show       # print the merged config (defaults + file + flags) as TOML
kleya config path       # print the path of the file that was actually loaded (or "<defaults>")
```

## Agent skill (optional)

If you drive kleya from a coding agent (Claude Code, Cursor, OpenCode, Codex), you can install a companion **skill** that teaches the agent the launch / connect / templates / unattended-handoff workflows so it stops reaching for raw `aws ec2` commands.

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://github.com/antstanley/kleya/releases/download/v0.1.0-rc.2/install-skill.sh | sh
```

The installer autodetects which agents you have configured (it looks for `~/.claude/`, `~/.cursor/`, `~/.config/opencode/`, `~/.agents/`, `~/.codex/`) and writes the skill to each one's native location. Override with `--target=claude,opencode` (comma-separated) or `--target=all`.

- `~/.claude/skills/using-kleya/SKILL.md` — Claude Code
- `~/.cursor/skills/using-kleya/SKILL.md` — Cursor (also picks up the Claude path)
- `~/.config/opencode/skills/using-kleya/SKILL.md` — OpenCode
- `~/.agents/skills/using-kleya/SKILL.md` — `.agents` folder spec (cross-agent fallback)
- `~/.codex/AGENTS.md` — Codex (appended between idempotent marker comments)

Restart your agent after install. To pin a specific version, pass `--version=v0.1.0-rc.2`.

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

```toml
default_region  = "eu-west-1"
default_profile = "default"

[defaults]
instance_type = "m8g.xlarge"
market        = "spot"       # or "on-demand"
spot_type     = "one-time"   # or "persistent"
ami_alias     = "amazon-linux-2023-arm64"

[bootstrap]
# Optional path to a custom user-data script. If set, install_ghostty_terminfo
# has no effect (the script is passed through verbatim, base64-encoded).
# user_data_path = "~/.config/kleya/bootstrap.sh"
install_ghostty_terminfo = true

[ssh]
user         = "ec2-user"
tmux         = true
tmux_session = "kleya"
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
| `KLEYA_PROFILE` | AWS profile to use (overrides `default_profile`) |
| `KLEYA_REGION` | AWS region (overrides `default_region`) |
| `KLEYA_LOG_FORMAT` | `text` (default) or `json` |
| `AWS_*` | Standard AWS SDK env vars (`AWS_ACCESS_KEY_ID`, `AWS_PROFILE`, etc.) |

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
| 7 | Key mismatch (local fingerprint differs from EC2) or key orphaned (EC2 key present, local pem missing) |
| 70 | AWS adapter error (SDK / network / API) |
| 74 | I/O error |
| 130 | Cancelled (Ctrl-C / SIGINT) |

## Troubleshooting

**`Adapter aws-ec2: ... no default VPC`.** kleya's zero-config path assumes a default VPC in the chosen region. Either create one (`aws ec2 create-default-vpc`) or specify `subnet_id` and `security_group_ids` in a `[[templates]]` block.

**`KeyMismatch`.** Your local `~/.config/kleya/keys/<name>.pem` fingerprint differs from what EC2 has registered. Either delete the local pem (kleya will treat as orphaned and you can re-import the EC2 public half manually), or delete the EC2 key pair (`aws ec2 delete-key-pair --key-name <name>`) and let kleya regenerate.

**`SshNotReady` after 180s.** The probe couldn't reach port 22. Check the security group allows your IP and that the instance actually started (`kleya list` or the EC2 console).

**`launch --connect` drops you mid-bootstrap.** Use `--wait-bootstrap` (the default when `--connect` is set unless `--no-wait-bootstrap` is also passed). The wait runs `cloud-init status --wait` over SSH before returning control.

## License

Dual-licensed under `Apache-2.0 OR MIT` per [`Cargo.toml`](Cargo.toml#L13). You may use this crate under the terms of either, at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for repo layout, tooling, conventions, and the release process.
