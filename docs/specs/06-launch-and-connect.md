# 06 — Launch and Connect

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

This page describes the runtime orchestration that turns `kleya launch` and `kleya connect` into a working remote shell. Each subcommand is a straight-line sequence of `CloudCompute` + `KeyStore` calls; the cross-cutting concerns (cancellation, idempotency, error mapping) are handled by the ports described in [04-provider-port.md](04-provider-port.md).

---

## Responsibilities

1. Build a `LaunchPlan` from CLI / config / defaults, validate it, and either print it (`--dry-run`) or execute.
2. Run the keypair lifecycle on every launch — drift catches as `Error::KeyMismatch` or `Error::KeyOrphaned`.
3. Ensure default resources (security group, template) idempotently, then `RunInstances` with the four management tags.
4. Wait for `state == Running` against a cancellable `Deadline`.
5. On `--connect` (and equivalent post-launch path), TCP-probe port 22, optionally wait for cloud-init, then `execvp` ssh.

---

## Zero-config defaults

Every option falls back so `kleya launch` with no flags and no config file works:

| Option | Default | Resolution path |
|---|---|---|
| `region` | `eu-west-1` | `--region` → `KLEYA_REGION` → `AWS_REGION` → `Config::default_region` |
| `profile` | `default` | `--profile` → `KLEYA_PROFILE` → `AWS_PROFILE` → `Config::default_profile` |
| `template` | `default` | `--template <name>` → built-in `"default"`; auto-created if missing |
| `instance_type` | `m8g.xlarge` | `--instance-type` → `[[templates]].instance_type` → `defaults.instance_type` |
| `market` | `spot` | `--market` → `defaults.market` |
| `ami_id` | resolved at launch | template's `ami_id`, else SSM `/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64` |
| `subnet_id` | first subnet of default VPC | `resolve_default_subnet()` — lexicographically first by AZ |
| `security_group_ids` | `kleya-default` SG | `ensure_default_security_group("kleya-default")`; opens `22/tcp` from `0.0.0.0/0` |
| `key_name` | `kleya-default` | `Config::keys.default_key_name`; generated if missing |
| Instance `Name` tag | `kleya-<adj>-<animal>` | `AdjAnimalIdGen::name()`; matches `^[a-z0-9][a-z0-9-]{0,62}$` |
| `user_data` | embedded `setup_devbox.sh.j2` | with ghostty terminfo block enabled |
| `ssh.user` | `ec2-user` | AL2023 |
| `ssh.tmux` | `true`, session `"kleya"` | overridable per-launch via `--no-tmux` / `--tmux-session` |
| `wait_timeout` | 600 s | `LAUNCH_WAIT_SECONDS_MAX` |
| `poll_interval` | 5 s | `LAUNCH_POLL_INTERVAL_SECONDS` |

The Decisions section of [00-overview.md](00-overview.md) records why the default SG opens `0.0.0.0/0`; tightening is a follow-up spec.

---

## `LaunchService::run` orchestration

[crates/kleya-core/src/commands/launch.rs](../../crates/kleya-core/src/commands/launch.rs).

```
1. build_plan(opts)
   ├── TemplateName  = opts.template_name OR "default"
   ├── InstanceName  = opts.instance_name OR id_gen.name()
   ├── KeyName       = config.keys.default_key_name (validated)
   └── AmiId         = compute.resolve_ami_alias(config.defaults.ami_alias)
                                              ↑ deterministic SSM lookup

2. if opts.dry_run: tracing::info!("dry-run plan", …); return Ok(None)

3. ensure_template(&plan)
   ├── ensure_keypair(&plan.key_name)             ← every launch (see lifecycle)
   ├── template_get_by_name(plan.template)
   │     └── Some(_) → return Ok(())              ← idempotent short-circuit
   ├── resolve_default_subnet()                   ← lexicographically first AZ
   ├── ensure_default_security_group("kleya-default")
   ├── render_user_data() → base64 (gzip+b64 or passthrough; see 05)
   ├── build TemplateSpec
   └── ensure_default_template(spec)

4. instance_launch(LaunchRequest {
       template, instance_name,
       instance_type_override, market_override, spot_type_override,
       extra_tags: [],
       key_name,
   })
   ↑ adapter adds Name + kleya:managed + kleya:template + kleya:key tags

5. instance_wait_running(inst.id, Deadline {
       timeout: 600s, poll_interval: 5s, cancel: opts.cancel,
   })
   ↑ wait_or_cancel observes the token without waiting a full interval
```

The orchestration is intentionally a straight-line sequence — there are no nested conditionals, no rollback on partial failure. A failure mid-way leaves AWS in whatever state it reached; the operator sees the partial result via `kleya list` and can `terminate` or re-`launch` from there.

`kleya launch --dry-run` prints the resolved plan via a single `tracing::info!` event and exits 0 without provisioning. The fields logged are `template`, `instance`, `key`, `ami` — the four resolution decisions a careful operator wants to see before committing.

---

## Keypair lifecycle

`LaunchService::ensure_keypair` evaluates the four-way `(KeyStore::exists, CloudCompute::keypair_fingerprint)` match on **every** launch. The full lifecycle table is in [01-domain-model.md](01-domain-model.md#lifecycle-keypair). Summary:

```
match (local_exists, cloud_fingerprint) {
    (true,  Some(cloud_fp)) => assert local_fp == cloud_fp,        // KeyMismatch on diff
    (true,  None)           => ensure_default_keypair(local_pub),  // re-register
    (false, Some(_))        => Error::KeyOrphaned,
    (false, None)           => generate Ed25519 → import,
}
```

Cost of running this every launch: one `DescribeKeyPairs` call (~50 ms), negligible against the launch wall clock. Benefit: drift introduced out-of-band (key rotated in EC2 by a teammate, local pem clobbered, etc.) surfaces at the next launch rather than as a cryptic `RunInstances` failure.

---

## `ConnectService::plan` orchestration

[crates/kleya-core/src/commands/connect.rs](../../crates/kleya-core/src/commands/connect.rs).

```
1. validate(opts.tmux_session) if present (regex ^[a-z0-9_-]{1,63}$)

2. resolve(opts) → Instance
   ├── opts.explicit_instance_id    → instance_describe
   ├── opts.handle starts with "i-" → instance_describe
   └── otherwise                    → instance_list(filter = name + managed_only)
                                          ├── 0 matches  → Error::InstanceNotFound
                                          ├── 1 match    → use it
                                          └── >1 matches → Error::AmbiguousHandle

3. resolve_key(&inst) → KeyName
   ├── tag kleya:key present    → that value
   ├── tag kleya:managed=true   → config.keys.default_key_name (validated)
   └── neither                  → Error::ConfigInvalid("not managed by kleya …")

4. key_path = key_store.private_path(&key_name)     ← mode 0600 asserted

5. endpoint = inst.public_dns or Error::ConfigInvalid("no public DNS")

6. argv = build_argv(endpoint, key_path, opts)      ← snapshot-stable

7. return ConnectPlan { argv, instance_id, endpoint, key_path }
```

The CLI's `Cmd::Connect` arm then:

- If `--print`: shell-quote `plan.argv` and emit on stdout, exit 0.
- Otherwise: `probe_ssh_ready(endpoint, instance_id, cancel)` then `Command::new(plan.argv[0]).args(&plan.argv[1..]).exec()`. The `exec()` call replaces the kleya process with `ssh`; on failure (`NotFound`, EACCES, etc.) the syscall returns and we map to `Error::Io`.

### Argv assembly

`build_argv` is one ~30-line function; the test `build_argv_includes_tmux_by_default` snapshots the canonical form:

```
ssh -i <key_path>
    -o StrictHostKeyChecking=accept-new
    -o ServerAliveInterval=30
    -o ConnectTimeout=10
    [config.ssh.extra_args…]
    -t <config.ssh.user>@<endpoint>
    tmux new-session -A -s <session>           # only if ssh.tmux && !--no-tmux
```

`-A` attaches to or creates the named session; `-t` forces a TTY. `--no-tmux` drops the trailing `tmux …` argv. `--tmux-session <s>` substitutes the session name after passing the `^[a-z0-9_-]{1,63}$` regex.

### Key-name fallback semantics

`resolve_key` enforces the **managed gate**: the `keys.default_key_name` config fallback only applies when the instance carries `kleya:managed=true`. An instance with neither `kleya:key` nor `kleya:managed=true` returns `Error::ConfigInvalid { reason: "instance <id> not managed by kleya; pass --instance-id and configure a key" }`. The error message tells the operator how to recover (`--instance-id` plus a known key path).

An empty `keys.default_key_name` is a secondary guard: if the fallback path is taken but the config value is blank, surface the longer "no kleya:key tag … re-launch via kleya launch" message rather than letting a zero-length `KeyName` propagate.

---

## SSH readiness probe

[crates/kleya-cli/src/ssh_probe.rs](../../crates/kleya-cli/src/ssh_probe.rs).

```rust
pub async fn probe_ssh_ready(
    endpoint: &str,
    instance: &InstanceId,
    cancel: &CancellationToken,
) -> Result<()>;
```

Named constants from `kleya_core::limits`:

| Constant | Value |
|---|---|
| `SSH_PROBE_PORT` | 22 |
| `SSH_PROBE_TIMEOUT_SECONDS` | 180 |
| `SSH_PROBE_INTERVAL_SECONDS` | 3 |
| `SSH_PROBE_TCP_TIMEOUT_MS` | 2 000 |

```
loop {
    if elapsed >= TIMEOUT → Error::SshNotReady { instance, elapsed_seconds }
    try TcpStream::connect("<endpoint>:22") with 2s timeout
        Ok(Ok(_)) → return Ok(())
        else      → wait_or_cancel(INTERVAL, cancel)
                       true  → Error::Cancelled { instance }
                       false → next iteration
}
```

Two assertions per call: `assert!(!endpoint.is_empty(), …)` and `const { assert!(SSH_PROBE_INTERVAL_SECONDS > 0, …) }`. The `wait_or_cancel` helper handles the `tokio::select!` over `cancel.cancelled()` and `tokio::time::sleep(interval)` — cancellation is observed without waiting for the next interval.

---

## Cloud-init wait

`probe_ssh_ready` confirms only that port 22 is reachable — the instance may still be mid-bootstrap. When the operator wants to land on a fully bootstrapped box, they ask for the cloud-init wait:

- `--wait-bootstrap` runs `cloud-init status --wait` over SSH explicitly.
- `--connect` implies the wait unless `--no-wait-bootstrap` is also passed.

```
ssh -i <key_path> -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 \
    <ssh.user>@<endpoint> cloud-init status --wait
```

A non-zero exit from `cloud-init status --wait` is surfaced as `Error::ConfigInvalid { reason: "cloud-init wait failed (exit <code>)" }`. The effective decision is centralised in `dispatch::effective_wait_bootstrap`:

```rust
args.wait_bootstrap || (args.connect && !args.no_wait_bootstrap)
```

Four unit tests pin the truth table; see [02-cli-surface.md](02-cli-surface.md) for the user-facing flag semantics.

---

## Launch ↔ connect chaining

`kleya launch` does **not** auto-attach by default. The four meaningful invocations:

| Flags | Behaviour |
|---|---|
| no flags | Wait for `Running`, print `id name dns`, exit 0. Operator runs `kleya connect <name>` next. |
| `--wait-bootstrap` | After `Running`, run `cloud-init status --wait` over SSH, exit 0. Still does not connect. |
| `--connect` | After `Running` + cloud-init (implicit wait), TCP-probe 22, then `execvp` ssh. |
| `--connect --no-wait-bootstrap` | After `Running`, TCP-probe 22 only, then `execvp` ssh. |

The `--connect` path reuses the same `ConnectService::plan` as `kleya connect`, with `explicit_instance_id` set to the just-launched id so the resolution skips the tag query.

---

## Cancellation timeline

```
operator presses Ctrl-C
       │
       ▼
SIGINT delivered to kleya
       │
       ▼
tokio::signal::ctrl_c() future resolves
       │
       ▼
cancel.cancel()                            ← single shared CancellationToken
       │
       ├──────────────┬───────────────────┬─────────────────────┐
       ▼              ▼                   ▼                     ▼
instance_wait_     probe_ssh_      Other tokio futures   (no-op for fast paths)
running's          ready's
wait_or_cancel     wait_or_cancel
       │              │
       ▼              ▼
Error::Cancelled { instance }              ← short-circuits any pending sleep
       │
       ▼
exit_code::code_for(&e) = 130
       │
       ▼
process exits with code 130
```

The cancel token does **not** terminate the AWS-side resources that may be in flight. Operators see whatever partial state landed via `kleya list` and `terminate` it manually. This is intentional — a Ctrl-C halfway through `RunInstances` may leave a spot request open; surfacing that to the operator is better than trying to roll back.

---

## Terminate

`TerminateService::terminate_by_handle(&str)` (in [commands/terminate.rs](../../crates/kleya-core/src/commands/terminate.rs)) reuses the same handle-resolution outcomes as `connect`:

- Handle starting with `i-` → `InstanceId::new(handle)`.
- Otherwise → `instance_list(InstanceFilter { name, managed_only: true })` → exactly-one or `InstanceNotFound` / `AmbiguousHandle`.

Then `instance_terminate(id)`. The CLI prompts interactively unless `--yes` is passed.

---

## Assumptions and open questions

**Assumptions**

- The launched instance gets a public DNS name. Within the default VPC of a commercial region this is true; in custom-VPC setups it may not be, and `connect` surfaces `Error::ConfigInvalid { reason: "instance <id> has no public DNS" }`.
- `ssh-keyscan`-style hostkey-strict mode is not the desired UX. `StrictHostKeyChecking=accept-new` accepts the first key seen and rejects on later mismatches — operators must clear `~/.ssh/known_hosts` to re-accept a recreated instance with a reused name.
- `cloud-init status --wait` is available on the AL2023 image; the bootstrap script does not need to install it.

**Decisions**

- *No state file.* **The instance carries its own metadata via the four `kleya:*` tags.** A local state file would diverge under multi-terminal use and would need lock-file handling on `terminate`. Tags survive instance restarts and are read-only to the operator unless they explicitly `aws ec2 create-tags`.
- *Run `ensure_keypair` every launch, not only on first run.* **Negligible round-trip cost, large UX win for drift.** A previous design had a first-run short-circuit; we deliberately removed it.
- *`--connect` implies cloud-init wait.* **Operators want to land on a finished box; the implicit wait avoids a confused "why is `claude` not on the path?" report.** `--no-wait-bootstrap` is the explicit opt-out.
- *`execvp` rather than spawn-and-wait.* **Cleaner Ctrl-C behaviour — ssh becomes pid 1 of the operator's process, no zombie kleya hanging around.** Means `connect` exits with ssh's exit code, which is the right thing for shell-driven invocations.
- *Probe TCP-22 rather than full SSH handshake.* **Faster, simpler, and good enough.** SSH key exchange adds a couple seconds without changing the answer for a freshly booted instance.

**Open questions**

- *Connect retry on `cloud-init status --wait` transient failure.* Today a non-zero exit is fatal. cloud-init's own status reporting is reliable in our experience, but if we see flaky failures, a single retry with a short delay would be cheap.
- *Auto-tightening the default SG.* The current `0.0.0.0/0` ingress is the simplest thing that works; gating to the operator's current egress IP would need either an HTTP call to a reflection service or a `--my-ip` flag. Both feel like a separate spec.
- *Multi-instance launches.* Today `RunInstances` is called with `min_count=max_count=1`. If we expose `--count`, the orchestration above becomes a loop over a list of instances — the keypair lifecycle still runs once, but the tag generation and wait need fan-out. Deferred.
