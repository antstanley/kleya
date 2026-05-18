---
name: using-kleya
description: Use whenever the user wants to spin up, attach to, list, or tear down an AWS EC2 dev box via the `kleya` CLI — including phrasings like "give me a dev box", "launch a sandbox", "spin up an aarch64 box", "claude code on EC2", "kleya launch", "connect to my kleya box", "terminate my kleya instance", or any time they mention kleya, spot dev boxes, or Claude-Code-ready EC2 environments. Also use when configuring kleya (templates, AMIs, instance types, regions, keys, `~/.config/kleya/config.toml`) or diagnosing kleya errors (KeyMismatch, SshNotReady, no default VPC, exit code 5/7/70). Prefer this skill over reaching for `aws ec2 …` directly whenever kleya can do the job.
---

# Using kleya

`kleya` is a Rust CLI that bootstraps AWS EC2 spot instances as Claude-Code-ready dev boxes. Zero-config: `kleya launch` with no flags provisions an Amazon Linux 2023 ARM spot instance, installs zsh/oh-my-zsh/tmux/git/rust/node/jj/python/uv/Claude Code, and prints an `ssh` command to attach. You can also let kleya do the SSH for you with `--connect`.

This skill is for an agent driving kleya locally — launching, attaching, listing, terminating, and managing templates/config. Not for agents who already landed on a kleya box.

## When to reach for kleya vs. the AWS CLI

If the user wants a dev box, sandbox, throwaway VM, or "a place to run Claude Code", default to `kleya launch` rather than hand-rolling `aws ec2 run-instances`. Kleya handles the things people forget: the AL2023 ARM AMI alias, default VPC + subnet lookup, security group with their public IP allowed, key pair creation, user-data bootstrap, tags so `kleya list` finds it later, and waiting for cloud-init to actually finish before handing them a shell.

Only fall back to raw `aws` when the user asks for something kleya doesn't cover (e.g. attaching an existing EBS volume, joining a non-default VPC without writing a template first, IAM, S3, …).

## The mental model in one paragraph

A kleya instance is an EC2 spot instance tagged `kleya:managed=true`, `kleya:name=<name>`, `kleya:key=<keyname>`. The name is the handle for every subsequent command. `kleya list` shows your fleet (filtered by those tags). `kleya connect <name>` resolves the name → instance → public IP → key path, then `exec`s `ssh ... 'tmux new-session -A -s kleya'`. Because tmux attaches by name, every reconnect drops you back into the same session. Termination is `kleya terminate <name>`.

Templates are saved sets of launch defaults (instance type, AMI, key, security groups, subnet, tags) stored in `~/.config/kleya/config.toml` under `[[templates]]`. `kleya launch` without `--template` uses a `default` template that kleya auto-creates on first run.

## The lifecycle commands

### Launch

```
kleya launch [--template <name>] [--name <name>]
             [--instance-type <type>] [--market spot|on-demand]
             [--connect] [--wait-bootstrap | --no-wait-bootstrap]
             [--dry-run]
```

Defaults: spot, AL2023 aarch64, the auto-`default` template. The name is auto-generated (`kleya-<adj>-<animal>`) if you don't supply one. **Print the assigned name back to the user** — they need it for every other command.

- `--connect` waits for SSH (port 22), then waits for cloud-init to finish, then `exec`s ssh into a tmux session. This is the right default for "give me a box and drop me in." Implies `--wait-bootstrap`.
- `--no-wait-bootstrap` (paired with `--connect`) skips the cloud-init wait. Only use when the user explicitly wants the SSH prompt before bootstrap finishes — note that Claude Code / rust / etc. won't be installed yet.
- `--dry-run` resolves the launch plan (AMI, subnet, SG, key, user-data size) and prints it without provisioning. Use this when the user wants to check what kleya is about to do, or when you suspect a config error before paying for an instance.
- `--instance-type` and `--market` override the template's value for a single launch (e.g. `--instance-type m7g.xlarge`, `--market on-demand`).

When launching without `--connect`, kleya prints an ssh command. Hand that to the user verbatim — don't rephrase it.

### Connect

```
kleya connect <name> [--print] [--no-tmux] [--tmux-session <s>] [--instance-id i-...]
```

The handle is the **name** (the tag), not the AWS instance id. If two instances share a name kleya exits with code 4 (`Handle ambiguous`) — resolve with `--instance-id i-...`. `--print` prints the ssh argv without executing, useful when the user wants to script around it or wire it into another tool. `--no-tmux` skips the `tmux new-session -A` wrapper if the user prefers a raw shell.

### List

```
kleya list [--json]
```

Shows everything tagged `kleya:managed=true` in the current region. Use `--json` when you need to parse it; otherwise the plain-text table is what the user wants. Empty output means no kleya instances exist in this region — check region first before declaring "no instances", since a box might exist in a different region.

### Terminate

```
kleya terminate <name> [--yes]
```

Interactive confirmation by default. Pass `--yes` only when the user has explicitly authorized non-interactive termination, or you've already confirmed with them in this turn. **Never** add `--yes` "to be helpful" — termination is irreversible and the confirmation is a deliberate safety belt.

## Templates and config

The simplest way to drive kleya is zero-config + per-launch flags. Reach for templates and config files when the user has settled into a repeatable setup.

### When to write a template vs. pass flags

- One-off ("just give me a box once"): flags only. `kleya launch --instance-type m7g.xlarge --market on-demand`.
- Repeated launches with the same shape: save a template once, then `kleya launch --template <name>` from then on.
- Different shapes for different jobs (e.g. a `gpu` box and a `cheap` box): one `[[templates]]` block per shape, named.

### Creating templates

```
kleya template create --name <n> [--ami <ami-id>] [--instance-type <t>]
                                   [--key-name <k>] [--user-data <path>]
kleya template update --name <n> [...same flags...]
kleya template list   [--json]
kleya template delete <name> [--yes]
```

`--user-data <path>` replaces the default bootstrap entirely — pass a path to a file kleya will base64-encode and submit verbatim. The default bootstrap installs the Claude-Code stack; supplying your own removes that. Warn the user when you swap in a custom user-data file unless they've explicitly opted out of the default bootstrap.

### Config file

Resolved in order: `--config <path>` flag → `KLEYA_CONFIG` env var → `~/.config/kleya/config.toml` (also accepts `.yaml`, `.json`, `.jsonc`).

A small useful example, in TOML:

```toml
default_region  = "eu-west-1"
default_profile = "default"

[defaults]
instance_type = "m8g.xlarge"
market        = "spot"
spot_type     = "one-time"     # or "persistent"
ami_alias     = "amazon-linux-2023-arm64"

[bootstrap]
install_ghostty_terminfo = true
# user_data_path = "~/.config/kleya/bootstrap.sh"   # overrides default bootstrap entirely

[ssh]
user         = "ec2-user"
tmux         = true
tmux_session = "kleya"
extra_args   = []              # appended verbatim to the ssh argv

[keys]
dir              = "~/.config/kleya/keys"
default_key_name = "kleya-default"

[[templates]]
name          = "gpu"
instance_type = "g6.xlarge"
# ami_id              = "ami-..."   # else resolved from ami_alias
# key_name            = "..."
# security_group_ids  = ["sg-..."]
# subnet_id           = "subnet-..."

[[templates.tags]]
key   = "Project"
value = "gpu-experiments"
```

Use `kleya config show` to print the merged config (defaults + file + flags) — this is the source of truth when you're not sure what kleya will actually do. `kleya config path` tells you which file kleya is reading (or `<defaults>` if none).

### Picking instance types

AL2023 ARM is the default AMI alias, so default to Graviton instance types unless the user has a reason for x86 (specific binaries, kernel modules, GPU). Reasonable defaults:

- General-purpose, cheap: `t4g.medium` or `t4g.large` (burstable, fine for editing + light compilation)
- General-purpose, steady: `m7g.xlarge` or `m8g.xlarge`
- Compute-heavy compilation: `c7g.2xlarge` or larger
- GPU: switch AMI to a deep-learning AL2023 build and pick `g6.xlarge` (or larger Grace-Hopper variants)

If the user says "cheap" without specifying, suggest `t4g.medium` on spot. If they say "fast" without specifying, suggest `m7g.xlarge` on spot.

## Errors and how to react

Kleya uses distinct exit codes — match the code, don't just grep stderr.

| Code | Meaning | What to do |
|---|---|---|
| 0 | Success | — |
| 1 | User-data exceeds limits | The bootstrap script is too big (EC2 caps user-data at 16 KiB). If the user supplied a custom `user_data_path`, trim it. Otherwise this is a kleya bug; report it. |
| 2 | Config invalid / pre-flight failed | Stderr names the field. Fix the config, or override via flag. |
| 3 | Instance not found | The handle didn't resolve. Check region, run `kleya list`, confirm spelling. |
| 4 | Handle ambiguous | Multiple instances share the name. Use `--instance-id i-...` to disambiguate. |
| 5 | SSH not ready within 180s | Port 22 unreachable. Likely security group doesn't allow your current public IP (it might have changed since launch), or the instance failed to come up — check `kleya list` and the EC2 console. |
| 6 | Launch wait timed out (600s default) | The instance didn't reach `running`. Often a spot capacity issue; retry, switch instance type, or try `--market on-demand`. |
| 7 | Key mismatch / orphaned key | Local `~/.config/kleya/keys/<name>.pem` fingerprint differs from EC2's. Either delete the local pem and re-import the public half, or `aws ec2 delete-key-pair --key-name <name>` and let kleya regenerate. Confirm with the user before deleting either side. |
| 70 | AWS adapter error (SDK / network / API) | Stderr has the AWS error. Common ones: `no default VPC` → either `aws ec2 create-default-vpc` or specify `subnet_id` + `security_group_ids` in a template. Throttling → retry. UnauthorizedOperation → check IAM. |
| 74 | I/O error | Usually filesystem (config file unreadable, keys dir missing). |
| 130 | Cancelled (Ctrl-C / SIGINT) | The user interrupted; don't retry without asking. |

### "No default VPC"

Common in fresh AWS accounts or non-default regions. Two fixes — prefer the first unless the user has a reason to keep a customized network setup:

1. `aws ec2 create-default-vpc` (in the target region). Takes a few seconds, then `kleya launch` works as-is.
2. Define a template with explicit `subnet_id` and `security_group_ids`.

### KeyMismatch / orphaned key

Don't auto-resolve. Walk the user through it: show them the local key path, the EC2 key name, and ask whether they want to (a) keep the EC2 key and re-import the local public half, or (b) delete the EC2 key and let kleya regenerate. Then act.

## Environment variables

Useful overrides that don't need a config file:

| Variable | Effect |
|---|---|
| `KLEYA_CONFIG` | Config file path (overrides default search) |
| `KLEYA_PROFILE` | AWS profile (overrides config `default_profile`) |
| `KLEYA_REGION` | AWS region (overrides config `default_region`) |
| `KLEYA_LOG_FORMAT` | `text` (default) or `json` — set to `json` when piping kleya output into other tools |
| `AWS_*` | Standard AWS SDK env vars (`AWS_PROFILE`, `AWS_REGION`, `AWS_ACCESS_KEY_ID`, …) |

`kleya --region <r>` flag wins over `KLEYA_REGION` wins over `default_region` in the config.

## Common workflows

### "Give me a dev box and drop me in"

```bash
kleya launch --connect
```

That's it. The user lands in a tmux session on a fully bootstrapped AL2023 ARM spot instance. Tell them: detach with Ctrl-b d; reattach with `kleya connect <name>` (substituting the name kleya printed).

### "I need an x86 box for this binary"

```bash
kleya launch --instance-type m7i.xlarge --connect
```

Add `ami_alias = "amazon-linux-2023-x86_64"` to the relevant template if this is going to be the user's regular setup.

### "I have three boxes running, which one's which?"

```bash
kleya list
```

If they don't recall what each was for, suggest tagging in future via a template with `[[templates.tags]]`.

### "Tear them all down"

Run `kleya list` first, show the user the names, then `kleya terminate <name>` for each — **with** the interactive confirm, unless they explicitly say "all of them, no prompts" (in which case `--yes`).

### "Hand off implementation to an unattended remote"

A common pattern: planning is done locally, the plan is on disk, and the user wants to spin up a kleya box and let Claude Code execute the plan on it while they go do something else. The local agent's job is to set this up so the work survives disconnect and the user can check on it later.

The shape of the handoff:

1. **Launch and connect:** `kleya launch --connect` (or `kleya launch` first and then `kleya connect <name>` once it prints the name). You need `--wait-bootstrap` to be in effect — the default when `--connect` is set — because Claude Code won't exist on the box until cloud-init finishes.
2. **Get the work onto the box.** Cleanest path: commit the plan + any working changes to a branch, push, and `git clone` (or `git pull`) on the remote. Alternative for unpushed scratch: `scp` the plan file across, or `cat plan.md | ssh ... 'cat > plan.md'`.
3. **Authenticate Claude Code on the remote.** The bootstrap installs `claude` but does not auth it. The agent needs `ANTHROPIC_API_KEY` (or whichever auth method the user prefers) exported on the remote — pass it across with `ssh -o SendEnv=ANTHROPIC_API_KEY` plus `AcceptEnv ANTHROPIC_API_KEY` on the remote, or have the user paste it once. Confirm with the user before exfiltrating credentials.
4. **Start claude in a detached tmux window so it survives disconnect.** From inside the connected tmux session:
   ```bash
   tmux new-window -d -n impl 'cd <repo> && claude --dangerously-skip-permissions -p "execute the plan at <path>" 2>&1 | tee impl.log'
   ```
   The `-d` flag creates the window without switching to it, so you can keep your shell. `tee impl.log` keeps a transcript on disk so the user can scrub progress on reconnect even if the tmux pane scroll buffer rolls over.
5. **Detach** (Ctrl-b d). The ssh connection closes, but tmux and the claude process keep running on the EC2 box. Print the reattach command for the user: `kleya connect <name>` then `tmux select-window -t impl` (or `Ctrl-b w` to pick from the window list).
6. **When the work is done**, the user reconnects, inspects `impl.log`, decides whether to push results back to origin, and then `kleya terminate <name>`.

Things that matter for this workflow:

- **Use a named tmux window, not a backgrounded shell.** `nohup … &` works but leaves no easy way to attach to the running process. tmux preserves the TTY so the user (or the remote agent's prompts) can interact when they reconnect.
- **Always pipe to a log file.** Without `tee`, the only record is the tmux scroll buffer, which is finite and lost on `kleya terminate`.
- **Don't `--dangerously-skip-permissions` without explicit user authorization.** It's load-bearing for unattended runs (the agent can't pause for permission prompts when no one is watching), but it disables a real safety net. Confirm before using it.
- **Don't terminate the box automatically.** Even if the run "completes," the user almost certainly wants to inspect output before throwing the disk away. End the handoff at "here's the reattach command"; let them tear down when ready.
- **Tell the user the exact reattach command** including the instance name, so they don't have to dig.

### "Offload a long, resource-heavy test suite"

When the user wants to run a test suite that's slow, CPU-bound, or memory-hungry — and they want their laptop back — kleya is a good fit. The shape:

1. **Size the box for the suite, not for editing.** CPU-bound Rust/Go/JS test suites want lots of cores and clock: `c7g.2xlarge` (8 vCPU / 16 GiB), `c7g.4xlarge` (16 / 32), or `c7g.8xlarge` (32 / 64). Memory-hungry suites want `m` or `r` family instead: `m7g.2xlarge` (8 / 32), `r7g.2xlarge` (8 / 64). Match the architecture to the binary — if the test artifacts must run on x86, use `c7i.*` and switch the AMI alias to `amazon-linux-2023-x86_64`.
2. **Spot vs on-demand: weigh eviction risk against cost.** Spot is 50–70% cheaper but the instance can vanish mid-run with two minutes' notice. For a 45-minute suite this is usually fine — restart on eviction. For a 6-hour fuzzing run, an eviction late in the run wastes hours; suggest `--market on-demand` unless the user is explicitly cost-optimizing.
3. **Launch and connect:** `kleya launch --instance-type c7g.4xlarge --connect`. If this is going to happen repeatedly, suggest saving a `test-runner` template instead.
4. **Get the code on the box.** `git clone <url> && cd <repo> && git checkout <branch>` is the cleanest path. If the user has uncommitted changes, push to a throwaway branch first, or `rsync -a --exclude target/ ./ <user>@<host>:~/repo/` — exclude build artifacts so the transfer doesn't include gigabytes of `target/` or `node_modules/`.
5. **Run the suite in a named tmux window with output captured to a file:**
   ```bash
   tmux new-window -d -n tests 'cargo nextest run --workspace 2>&1 | tee tests.log'
   ```
   The `-d` keeps your current pane focused. The log file is essential — tmux scroll buffer is finite and goes away with the instance.
6. **Detach (Ctrl-b d) and walk away.** Tell the user the reattach command: `kleya connect <name>`, then `Ctrl-b w` to find the `tests` window.
7. **Pull artifacts home before terminating.** Junit XML, coverage HTML, profile data — copy them back with `scp` or `rsync`:
   ```bash
   rsync -avz <user>@<host>:~/repo/target/nextest/ ./local-results/
   ```
   `kleya connect --print <name>` prints the resolved ssh argv (user, key, host) so you can construct the scp/rsync invocation without rederiving it.
8. **Then `kleya terminate <name>`.** Spot or on-demand, you're paying by the second; don't leave the box idling after the suite finishes.

Things to watch for:

- **EBS fills up surprisingly fast** during big builds. The default AL2023 root volume is generous but not unlimited. If a suite produces multi-GB artifacts (coverage profiles, fuzz corpora), check `df -h` periodically or `du -sh target/` after the run.
- **Avoid `cargo nextest` "fail-fast" on long runs** unless the user prefers it — they may want a full pass/fail breakdown rather than aborting on the first failure after 30 minutes of work.
- **Re-running the same suite tomorrow?** Save the template. `kleya template create --name test-runner --instance-type c7g.4xlarge` (plus any tags/key/AMI) so next time it's `kleya launch --template test-runner --connect`.

### "Set up kleya in a new AWS account"

1. Verify default VPC: `aws ec2 describe-vpcs --filters Name=is-default,Values=true`. Create one if missing.
2. Verify credentials: `aws sts get-caller-identity`.
3. Verify IAM has `ec2:*` and `ssm:GetParameter`.
4. `kleya launch --dry-run` to confirm the resolution chain works without provisioning.
5. `kleya launch --connect`.

## Things to be careful about

- **Termination is irreversible.** Spot instances don't have stop/start, only terminate. EBS data goes with them. Don't pass `--yes` autonomously.
- **The `default` template is auto-created on first launch.** If the user wants to customize it, they can `kleya template update --name default ...` or edit the config file directly. Do not delete it without confirming.
- **Regions are sticky per command.** `kleya list` only shows the current region. If the user thinks an instance is missing, check the other regions they might have launched in (`KLEYA_REGION=us-east-1 kleya list`, etc.).
- **`--no-wait-bootstrap` is a footgun.** Without it, `--connect` guarantees the user lands on a fully bootstrapped box. With it, they may see "command not found: claude" because cloud-init is still running.
- **Spot capacity can fail.** If launch times out (exit 6), retry once, then suggest a smaller instance type or `--market on-demand`.

## Quick reference: every command at a glance

```bash
kleya launch [--template <n>] [--name <n>] [--instance-type <t>]
             [--market spot|on-demand] [--connect]
             [--wait-bootstrap|--no-wait-bootstrap] [--dry-run]
kleya list   [--json]
kleya connect <name> [--print] [--no-tmux] [--tmux-session <s>] [--instance-id i-...]
kleya terminate <name> [--yes]

kleya template create --name <n> [--ami <id>] [--instance-type <t>]
                                  [--key-name <k>] [--user-data <path>]
kleya template update --name <n> [...]
kleya template list   [--json]
kleya template delete <name> [--yes]

kleya config show
kleya config path

# global flags valid on every command
--config <path> --profile <p> --region <r> -v|--verbose
--log-format text|json
```
