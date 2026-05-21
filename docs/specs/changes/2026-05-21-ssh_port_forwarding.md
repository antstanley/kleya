# Change: Generic SSH port (and path) forwarding

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley · **Target:** Repo-wide

`kleya connect` (and the `--connect` arm of `kleya launch`) will be able to **forward arbitrary ports and unix sockets to and from the remote dev box** — local→remote (`-L`) and remote→local (`-R`) — plus best-effort directory sync, configured once in `[ssh.forward]` or passed per-invocation. The port cases are a thin pass-through over the existing `ssh` argv builder; directory sync has no native SSH primitive and ships as a one-shot `rsync` push.

Sibling changes: [2026-05-21-credential_forwarding.md](2026-05-21-credential_forwarding.md) forwards the operator's *credentials*, and [2026-05-21-env_var_forwarding.md](2026-05-21-env_var_forwarding.md) forwards *environment variables*. This change is the generic transport layer; the credential change borrows one mechanism from here (its no-disk HTTPS-git credential-helper proxy runs over an `-R` unix socket defined below).

---

## Motivation

An agentic session on the box routinely needs network paths that `ssh user@host` alone does not give: reaching a service running on the box from the laptop (`-L`: a dev server, a database the box spun up, a debugger port), exposing a local service to the box (`-R`: a database or API on the operator's laptop, a local LLM endpoint, or the credential-helper socket the credential change relies on), and moving a directory up to the box (datasets, a local checkout, fixtures).

`ssh` supports all of the port cases natively, and kleya already builds the `ssh` argv ([06-launch-and-connect.md](../06-launch-and-connect.md)), so this is mostly surfacing `-L`/`-R` through kleya's config and flags with sane defaults and validation. Directories have no native SSH primitive: directory "forwarding" means **sync up** (`rsync-push`), not **mirror live**, until an `sshfs` reverse-mount lands later. The scope is deliberately small — no long-running daemon or reconnect supervisor; forwards live for the lifetime of the `ssh` process kleya `exec`s, exactly as native `ssh -L/-R` do.

---

## Affected spec pages

| Canonical page | Nature of change |
|---|---|
| [`docs/specs/03-configuration.md`](../03-configuration.md) | Add `[ssh.forward]` block (port + path arrays), defaults rows, validation rules |
| [`docs/specs/06-launch-and-connect.md`](../06-launch-and-connect.md) | Add `-L`/`-R` argv placement, the pre-exec `rsync` step, and forward resolution in `ConnectService::plan` |
| [`docs/specs/02-cli-surface.md`](../02-cli-surface.md) | Add `-L` / `-R` / `--forward-path` flags to `connect` and `launch` |
| [`docs/specs/05-bootstrap-rendering.md`](../05-bootstrap-rendering.md) | Ensure `rsync` is installed (and `sshfs`, when that mode lands) |
| [`canonical-types.schema.json`](../canonical-types.schema.json) | Add `SshForwardCfg`, `PortForward`, `PathForward` `$defs`; add optional `forward` to `SshCfg` |

---

## Proposed changes

One subsection per affected page. Each block is the prose as it should read once merged.

### `docs/specs/03-configuration.md` → Canonical schema (Modify)

Add an `[ssh.forward]` block to the canonical TOML, after `[ssh].extra_args`:

> ```toml
> # Arbitrary port / socket forwards (native -L / -R), applied on every connect.
> [[ssh.forward.port]]
> direction = "local"          # local (-L) | remote (-R)
> listen    = "127.0.0.1:5432" # "host:port" or a unix socket path; bare port => loopback
> connect   = "localhost:5432" # "host:port" or a unix socket path
>
> [[ssh.forward.port]]
> direction = "remote"
> listen    = "127.0.0.1:8080"
> connect   = "localhost:8080"
>
> # Arbitrary directory forwards (best-effort).
> [[ssh.forward.path]]
> local  = "~/datasets"
> remote = "~/datasets"
> mode   = "rsync-push"        # rsync-push (one-shot up) | sshfs (reverse mount, future)
> ```
>
> `listen`/`connect` mirror OpenSSH's `-L`/`-R` argument grammar so the mental model transfers directly. A bare port (`"5432"`) binds the loopback interface — never `0.0.0.0` — unless an explicit bind address is given. `ssh.forward` and its `port`/`path` arrays are optional; an absent block means no forwards, preserving zero-config launch. All structs keep `#[serde(deny_unknown_fields)]`.

### `docs/specs/03-configuration.md` → Validation (Modify)

> For `[ssh.forward]`:
> - Each `port.direction` is `"local" | "remote"`.
> - Each `port.listen` / `port.connect` parses as either `[bind:]port` (port in `1..=65535`) or an absolute unix-socket path; a bare port binds loopback.
> - Each `path.mode` is `"rsync-push"` (`"sshfs"` is reserved and rejected until that mode ships).
> - Malformed entries raise `Error::ConfigInvalid` (exit 2) at plan time, naming the offending field.

### `docs/specs/06-launch-and-connect.md` → Argv assembly (Modify)

> `-L`/`-R` forwards are appended to the argv after the `-o` options and `extra_args`, before the `-t <user>@<endpoint>` tail:
> ```
> ssh -i <key_path>
>     -o StrictHostKeyChecking=accept-new …
>     [config.ssh.extra_args…]
>     [-L [bind:]lport:rhost:rport]…        # one per local forward
>     [-R [bind:]rport:lhost:lport]…        # one per remote forward
>     -t <config.ssh.user>@<endpoint>
>     [tmux …]
> ```
> Because kleya `exec`s `ssh`, the forwards live and die with that process — no supervisor, matching native `ssh` semantics. A bare-port `listen` binds `127.0.0.1`; a non-loopback bind must be explicit, and `-R` to a public bind additionally needs `GatewayPorts` on the box (off by default; kleya does not enable it implicitly).

### `docs/specs/06-launch-and-connect.md` → `ConnectService::plan` orchestration (Modify)

> After the argv is built, directory forwards run **before** the interactive `exec`:
> - `rsync-push`: `rsync -az -e "ssh -i <key> -o …" <local>/ <user>@<endpoint>:<remote>/`, reusing the same key and host-key policy as the connect. A non-zero `rsync` exit aborts the connect with `Error::ConfigInvalid` rather than silently dropping into the shell.
> - `sshfs` (future): mount the operator's local dir on the box via an `-R`-tunneled `sshfs`; requires `sshfs` on the box and is out of this change.
>
> Forward resolution merges `[ssh.forward]` config with the additive `-L`/`-R`/`--forward-path` flags; the resolved set is part of the snapshot-stable `ConnectPlan` so `--print` can show it.

### `docs/specs/02-cli-surface.md` → `kleya connect` / `kleya launch` (Add)

> | Flag | Effect |
> |---|---|
> | `-L <[bind:]lport:rhost:rport>` | Ad-hoc local forward, repeatable; passed straight through to `ssh -L` |
> | `-R <[bind:]rport:lhost:lport>` | Ad-hoc remote forward, repeatable; passed straight through to `ssh -R` |
> | `--forward-path <local:remote>` | Ad-hoc directory sync, repeatable |
>
> Ad-hoc flags are additive over `[ssh.forward]` config. `-L`/`-R` use the exact OpenSSH spelling so muscle memory and copy-paste from `ssh` docs work unchanged. `--print` additionally lists every `-L`/`-R` and path sync the plan would set up.

### `docs/specs/05-bootstrap-rendering.md` → Template body (Modify)

> The bootstrap installs `rsync` so `--forward-path` / `[[ssh.forward.path]]` (`mode = "rsync-push"`) works on a fresh box. No `sshd_config` change is required for `-L`/`-R`: `AllowTcpForwarding` and `AllowStreamLocalForwarding` default to `yes` on the box's stock `sshd`. (`sshfs`, when that mode lands, additionally installs `sshfs` and sets `user_allow_other` where a shared mount is needed.)

---

## Type changes

Fragment for the `[ssh.forward]` config. Folds into `canonical-types.schema.json` on merge. `forward` is added to `SshCfg.properties` but **not** to its `required` array, so existing configs keep validating and the round-trip property test in `03` still holds.

```json
{
  "$comment": "Fragment for 2026-05-21-ssh_port_forwarding. Folds into canonical-types.schema.json on merge.",
  "$defs": {
    "SshForwardCfg": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "port": { "type": "array", "items": { "$ref": "#/$defs/PortForward" }, "default": [] },
        "path": { "type": "array", "items": { "$ref": "#/$defs/PathForward" }, "default": [] }
      }
    },
    "PortForward": {
      "type": "object",
      "additionalProperties": false,
      "required": ["direction", "listen", "connect"],
      "properties": {
        "direction": { "type": "string", "enum": ["local", "remote"] },
        "listen":    { "type": "string", "description": "[bind:]port or a unix-socket path; bare port binds loopback." },
        "connect":   { "type": "string", "description": "host:port or a unix-socket path." }
      }
    },
    "PathForward": {
      "type": "object",
      "additionalProperties": false,
      "required": ["local", "remote", "mode"],
      "properties": {
        "local":  { "type": "string" },
        "remote": { "type": "string" },
        "mode":   { "type": "string", "enum": ["rsync-push", "sshfs"], "description": "sshfs reserved; rejected until that mode ships." }
      }
    },
    "SshCfg": {
      "$comment": "Modified: add optional `forward`. Shown with the new property only; merge into the existing SshCfg def, leaving `required` unchanged.",
      "properties": {
        "forward": { "$ref": "#/$defs/SshForwardCfg" }
      }
    }
  }
}
```

---

## Implementation notes

Pointers for the implementing agent; ordering matters because the argv builder is snapshot-tested.

```
1. Add SshForwardCfg / PortForward / PathForward to crates/kleya-core/src/config.rs,
   with `forward: Option<SshForwardCfg>` on SshCfg (serde default None). Extend Config::validate
   with the listen/connect/mode rules (mirror the existing market/spot_type checks).
2. Parse + validate the listen/connect grammar in kleya-core (a small parse fn returning
   ConfigInvalid on malformed specs) — do not defer to ssh's usage error post-exec.
3. Extend build_argv in crates/kleya-core/src/commands/connect.rs:140 to append -L/-R after
   extra_args and before the -t tail. Update the build_argv_includes_tmux_by_default snapshot
   and add a forwards snapshot test.
4. Add the pre-exec rsync step in the Cmd::Connect arm (crates/kleya-cli) using the same key
   and host-key policy; map non-zero exit to ConfigInvalid.
5. Add -L/-R/--forward-path to crates/kleya-cli/src/clap_args.rs (repeatable Vec<String>),
   additive over config.
6. Add `rsync` to the bootstrap package list in the setup_devbox.sh.j2 template
   (crates/kleya-core/src/bootstrap/), no render-var gate needed.
```

References: OpenSSH `ssh(1)` `-L`/`-R` grammar; `AllowStreamLocalForwarding` (unix-socket forwarding, OpenSSH ≥ 6.7).

---

## Merge plan

1. Apply each `Proposed changes` block to its canonical page; bump each touched page's `**Date:**` to the merge date.
2. Fold the `Type changes` `$defs` into `canonical-types.schema.json` (`SshForwardCfg`/`PortForward`/`PathForward`, and the `forward` property on `SshCfg` — leave `SshCfg.required` unchanged).
3. No new canonical page is added.
4. Flip this file's `**Status:**` to `Merged`, add `**Merged:** YYYY-MM-DD`, and move it to `docs/specs/changes/merged/`.
5. Update `docs/README.md`: remove this file from the pending Change-specs list.

---

## Assumptions and open questions

**Assumptions**

- The box's stock `sshd` keeps `AllowTcpForwarding` and `AllowStreamLocalForwarding` at their `yes` defaults; kleya does not edit `sshd_config` for `-L`/`-R`.
- The operator's local `ssh` and `rsync` honour the same `-i <key>` / host-key options kleya passes; `rsync -e` reuses that exact ssh invocation.
- Forwards sharing the `exec`d `ssh` process lifetime is acceptable — there is no requirement to survive a dropped connection.

**Decisions**

- *`-L`/`-R` use OpenSSH spelling verbatim.* **Lowest-surprise surface; copy-paste from `ssh` docs and existing habits transfer.** Kleya validates and passes through rather than inventing a new grammar.
- *Loopback bind by default.* **A bare port must never silently listen on all interfaces** — the default dev box sits on a public subnet ([06](../06-launch-and-connect.md)).
- *No reconnect supervisor.* **Forwards share the lifetime of the `exec`d `ssh`, matching native semantics;** kleya does not add a daemon to keep them alive across drops.
- *Directory "forwarding" is rsync-push first.* **No native SSH primitive exists for a live mirror; `sshfs` is deferred** to avoid the dependency and its failure modes in the first cut.
- *Validation before exec.* **Malformed `listen`/`connect`/path specs fail at plan time (`ConfigInvalid`, exit 2)** rather than producing an opaque `ssh` usage error after the process is replaced.

**Open questions**

- *Persisted `-R` for the credential-helper proxy.* The [credential-forwarding](2026-05-21-credential_forwarding.md) change wants a standard `-R` unix socket for no-disk HTTPS git. Should that socket be a first-class named entry here, or owned entirely by the credential change?
- *`autossh`-style resilience.* Long agentic sessions outlive flaky networks; native `ssh` forwards die on disconnect. Worth an opt-in keepalive/reconnect wrapper, or explicitly out of scope?
- *Live directory sync.* Is `sshfs` reverse-mount actually wanted, or is `rsync-push` plus the operator's own watch tooling enough?
- *Per-template forwards.* Deferred, tracking the per-template-`[ssh]`-override open question in [03-configuration.md](../03-configuration.md); revisit if a "db box" vs. "code box" split emerges. (The [env-var-forwarding](2026-05-21-env_var_forwarding.md) change resolves that open question for `env` only; transport settings like forwards stay global.)
