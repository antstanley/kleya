# 02 — CLI Surface

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

The CLI is a `clap` derive tree rooted at [`clap_args::Cli`](../../crates/kleya-cli/src/clap_args.rs). `main.rs` (~30 lines) parses argv, installs the SIGINT handler, and hands a parsed `Cli` to `dispatch::run`. Every subcommand maps onto a service in `kleya_core::commands`.

---

## Responsibilities

1. Translate argv (and environment variables, and the config file) into a single validated `Config` plus a typed subcommand.
2. Build adapter instances (`AwsEc2`, `FsKeyStore`) and inject them into the matching service in `kleya_core::commands`.
3. Marshal service results into stdout, JSON when `--json` is present.
4. Map `kleya_core::Error` to the documented exit codes; print the error display via `tracing::error!` before exiting.
5. Propagate Ctrl-C through a `tokio_util::sync::CancellationToken` shared with every poll loop.

---

## Global flags

| Flag | Env | Default | Effect |
|---|---|---|---|
| `--config <path>` | `KLEYA_CONFIG` | search order in [03-configuration.md](03-configuration.md) | Force a specific config file |
| `--profile <p>` | `KLEYA_PROFILE` | `default` (from config) | Override the AWS profile (also honoured by the SDK via `AWS_PROFILE`) |
| `--region <r>` | `KLEYA_REGION` | `eu-west-1` (built-in default) | Override the AWS region |
| `-v` / `--verbose` | — | off | Repeatable: 1 → `debug`, 2+ → `trace` for `tracing-subscriber` |
| `--log-format text\|json` | `KLEYA_LOG_FORMAT` | `text` | Switch the `tracing-subscriber` layer |

Only the five `KLEYA_*` variables above (`KLEYA_CONFIG`, `KLEYA_PROFILE`, `KLEYA_REGION`, `KLEYA_LOG_FORMAT`) are read by kleya. Standard AWS SDK variables (`AWS_REGION`, `AWS_PROFILE`, `AWS_ACCESS_KEY_ID`, …) are honoured by the SDK itself.

---

## Subcommands

### `kleya launch`

Launches a spot (default) instance from a template.

```
kleya launch [--template <name>] [--name <instance-name>]
             [--instance-type <t>] [--market spot|on-demand]
             [--connect] [--wait-bootstrap] [--no-wait-bootstrap]
             [--dry-run]
```

| Flag | Effect |
|---|---|
| `--template <name>` | Use a config-defined template; default is `default` (auto-created if missing) |
| `--name <instance-name>` | Tag the instance with this `Name`; auto-generated `kleya-<adj>-<animal>` if omitted |
| `--instance-type <t>` | Override the template's `instance_type` for this run |
| `--market spot\|on-demand` | Override the template's market for this run |
| `--connect` | After SSH is reachable, `execvp` ssh. Implies `--wait-bootstrap` unless `--no-wait-bootstrap` is also passed |
| `--wait-bootstrap` | Run `cloud-init status --wait` over SSH before returning |
| `--no-wait-bootstrap` | Opt out of the cloud-init wait when `--connect` is set (TCP-22 readiness only) |
| `--dry-run` | Resolve and print the launch plan; exit 0 without provisioning |

**Effective wait rule** (single function `dispatch::effective_wait_bootstrap`, unit-tested):

```
wait = args.wait_bootstrap || (args.connect && !args.no_wait_bootstrap)
```

The three meaningful combinations: `--wait-bootstrap` alone keeps current behaviour without auto-attach; `--connect` alone implies wait; `--connect --no-wait-bootstrap` keeps `--connect` but skips the cloud-init poll. See [06-launch-and-connect.md](06-launch-and-connect.md) for the orchestration timeline.

### `kleya list`

```
kleya list [--json]
```

Lists instances tagged `kleya:managed=true` in the active region. Tab-separated columns by default: `id`, `name`, `state`, `public_dns`. `--json` emits a pretty-printed array via `serde_json`.

### `kleya connect`

```
kleya connect <name> [--print] [--no-tmux] [--tmux-session <s>] [--instance-id i-...]
```

Resolves the handle to a managed instance, reads `kleya:key` (with `keys.default_key_name` as the managed-instance fallback), TCP-probes port 22, and `execvp`s `ssh`. See [06-launch-and-connect.md](06-launch-and-connect.md) for argv assembly and probe timing.

| Flag | Effect |
|---|---|
| `--print` | Print the shell-quoted `ssh` argv and exit 0; do not invoke ssh |
| `--no-tmux` | Drop the trailing `tmux new-session -A -s <s>` argv |
| `--tmux-session <s>` | Override the configured tmux session name; validated against `^[a-z0-9_-]{1,63}$` |
| `--instance-id i-…` | Resolve directly by AWS instance id; useful for unmanaged instances |

### `kleya terminate`

```
kleya terminate <name> [--yes]
```

Same handle resolution as `connect`. Interactive `y/N` confirmation on stderr unless `--yes` is passed; an unconfirmed answer maps to `Error::ConfigInvalid { reason: "aborted: pass --yes to confirm" }`.

### `kleya template`

```
kleya template create --name <n> [--ami ami-...] [--instance-type <t>] [--key-name <k>] [--user-data <path>]
kleya template update --name <n> [--ami ami-...] [--instance-type <t>] [--key-name <k>] [--user-data <path>]
kleya template list   [--json]
kleya template delete <name> [--yes]
```

`update` builds the same `TemplateSpec` as `create`, then calls `template_update` against the existing template's id. Both `create` and `update` take `--name` as a flag (consistency across the surface — see Decisions). `delete` takes a positional name and the same `--yes` semantics as `terminate`.

### `kleya config`

```
kleya config show       # resolved Config serialised to TOML
kleya config path       # path of the loaded config file, or "<defaults; no file loaded>"
```

`show` round-trips the loaded `Config` through `toml::to_string_pretty`, so the output matches the canonical TOML schema in [03-configuration.md](03-configuration.md) regardless of which source format was used.

---

## Exit codes

Single function `exit_code::code_for(&Error) -> i32` in [exit_code.rs](../../crates/kleya-cli/src/exit_code.rs).

| Code | Cause |
|---|---|
| 0 | Success |
| 1 | `Error::UserDataTooLarge` (raw, gzip, or oversize-input ceiling exceeded) |
| 2 | `Error::ConfigInvalid` (pre-flight check failed, schema mismatch, tmux session regex, confirm-aborted) |
| 3 | `Error::InstanceNotFound` |
| 4 | `Error::AmbiguousHandle` (more than one managed instance matches the name) |
| 5 | `Error::SshNotReady` (TCP probe exhausted `SSH_PROBE_TIMEOUT_SECONDS`) |
| 6 | `Error::LaunchWaitTimeout` (state did not reach `Running` within `LAUNCH_WAIT_SECONDS_MAX`) |
| 7 | `Error::KeyMismatch` or `Error::KeyOrphaned` |
| 70 | `Error::Adapter` (`aws-ec2` SDK / network / API) — sysexits `EX_SOFTWARE` |
| 74 | `Error::Io` — sysexits `EX_IOERR` |
| 130 | `Error::Cancelled` (Ctrl-C / SIGINT, = 128 + SIGINT) |

Any clap parse error exits 2 via clap's own error path before `code_for` is reached.

---

## Output conventions

- Success messages go to **stdout**; one per command, machine-readable tab-separated for `list` and the `template list` default form.
- Telemetry / progress goes through `tracing` to **stderr**; JSON mode swaps the formatter only, the destination stays stderr.
- `--json` flags emit pretty-printed JSON arrays; the shape mirrors the relevant `kleya_core::model::*` Serialize impls.
- Confirmation prompts (`terminate`, `template delete` without `--yes`) write to **stderr** and read a single `y/N` line from **stdin**.
- Errors are rendered via `tracing::error!(error = %e)` and the process exits with `code_for(&e)`.

---

## SIGINT handling

`main.rs` spawns a `tokio::signal::ctrl_c()` listener and routes a single cancel into `dispatch::run`. The token is cloned into:

- `LaunchService::run` via `LaunchOpts.cancel` → `Deadline.cancel` for `instance_wait_running`.
- `ssh_probe::probe_ssh_ready` directly, as the third positional argument.

Both poll loops use `kleya_core::util::wait_or_cancel` to observe cancellation without waiting for the next interval to elapse, returning `Error::Cancelled { instance }` (exit code 130).

---

## Assumptions and open questions

**Assumptions**

- `ssh` is on the operator's `PATH`. `kleya connect` and the chained `--connect` path `execvp` it directly; an absent binary surfaces as `Error::Io(NotFound)` (exit 74).
- TTY is available for the interactive confirm prompts. There is no `--no-confirm` global flag; per-command `--yes` is the documented escape hatch.
- The operator's argv parser is GNU-compatible — `--flag=value` and `--flag value` are equivalent everywhere clap accepts them.

**Decisions**

- *Stable exit-code mapping.* **One integer per `Error` variant, never reused for a different cause.** Operators and agents script against these; reassigning a code is a breaking change.
- *`--name` flag on `template update`.* **Both `create` and `update` use `--name` rather than a positional argument.** Older drafts of the design doc had `update <name>` as positional; the consistency win for autocomplete and shell scripts beat the few keystrokes saved.
- *`--no-wait-bootstrap` only meaningful with `--connect`.* **The flag is silently a no-op when `--connect` is absent.** The alternative — requiring `--wait-bootstrap` to opt in even with `--connect` — surprised operators in the dogfood phase; the implicit wait matches the principle of "after `--connect` returns, you are on a finished box."
- *`Cmd::Config Show` round-trips through `toml::to_string_pretty`.* **`kleya config show` always prints TOML regardless of source format.** YAML/JSON/JSONC inputs are normalised to the canonical TOML for human review; serde guarantees the round-trip since all `Config*` structs derive `Serialize + Deserialize`.

**Open questions**

- *`--json` on `template list` and `list`.* Resolved: prioritise agent-friendly output. List commands ship a stable envelope (e.g. `{ "version": "1", "items": [...] }`) rather than raw `Vec<_>`, since kleya ships skills that consume this. Implementation is deferred to a follow-up task.
- *Auto-completion.* Resolved: add `clap_complete` generation now rather than deferring to v0.2.
