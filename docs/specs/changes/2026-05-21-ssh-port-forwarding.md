# Change — Generic SSH port (and path) forwarding

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley

A change spec proposing that `kleya connect` (and the `--connect` arm of `kleya launch`) be able to **forward arbitrary ports and unix sockets to and from the remote dev box** — local→remote (`-L`) and remote→local (`-R`) — plus best-effort directory sync, configured once or passed per-invocation.

Sibling change: [2026-05-21-credential-forwarding.md](2026-05-21-credential-forwarding.md) covers forwarding the operator's *credentials*. This spec is the generic transport layer; the credential change borrows one mechanism from here (the no-disk HTTPS-git credential-helper proxy runs over an `-R` socket defined below).

This file is a staging document. Once the work lands it merges into the canonical specs listed under [Canonical merge targets](#canonical-merge-targets) and is deleted.

---

## Motivation

An agentic session on the box routinely needs network paths that `ssh user@host` alone does not give:

- **Reach a service running on the box from the laptop** (`-L`): a dev server, a database the box spun up, a debugger port — open it locally without exposing it to the internet.
- **Expose a local service to the box** (`-R`): a database or API running on the operator's laptop, a local LLM endpoint, or the credential-helper socket the [credential-forwarding](2026-05-21-credential-forwarding.md) change relies on.
- **Move a directory up to the box**: datasets, a local checkout, fixtures.

`ssh` supports all of the port cases natively; kleya already builds the `ssh` argv ([06-launch-and-connect.md](../06-launch-and-connect.md)), so this is mostly surfacing `-L`/`-R` through kleya's config and flags with sane defaults and validation. Directories have **no** native SSH primitive and need a sync step.

---

## Goals

1. `-L` local forwards (local→remote), repeatable, TCP and unix-socket.
2. `-R` remote forwards (remote→local), repeatable, TCP and unix-socket.
3. Persisted, named-by-config forwards that apply on every `connect` to a box, plus ad-hoc per-invocation forwards.
4. Best-effort directory forwarding: `rsync-push` one-shot now, live `sshfs` reverse-mount deferred.

Non-goal: a long-running daemon or reconnect supervisor. Forwards live for the lifetime of the `ssh` process kleya `exec`s, exactly as native `ssh -L/-R` do.

---

## Feasibility

| Capability | Native SSH primitive | Verdict |
|---|---|---|
| local→remote TCP | `ssh -L [bind:]lport:rhost:rport` | trivial |
| remote→local TCP | `ssh -R [bind:]rport:lhost:lport` | trivial |
| unix-domain sockets (either direction) | `-L`/`-R` accept socket paths | supported, OpenSSH ≥ 6.7 |
| live directory mirror | **none** | needs `sshfs` (reverse mount over `-R`) — deferred |
| one-shot directory copy | `rsync`/`scp` (not a "forward") | ship as `rsync-push` |

The first three are a thin pass-through over the existing argv builder. Directory "forwarding" is the only item without a native primitive; the spec is explicit that it means **sync up**, not **mirror live**, until `sshfs` lands.

---

## Proposed config surface

New `[ssh.forward]` block carrying arrays of forwards that apply on every connect:

```toml
# Arbitrary port / socket forwards (native -L / -R)
[[ssh.forward.port]]
direction = "local"          # local (-L) | remote (-R)
listen    = "127.0.0.1:5432" # "host:port" or a unix socket path; bare port => loopback
connect   = "localhost:5432" # "host:port" or a unix socket path

[[ssh.forward.port]]
direction = "remote"
listen    = "127.0.0.1:8080"
connect   = "localhost:8080"

# Arbitrary directory forwards (best-effort)
[[ssh.forward.path]]
local  = "~/datasets"
remote = "~/datasets"
mode   = "rsync-push"        # rsync-push (one-shot up) | sshfs (reverse mount, future)
```

`listen`/`connect` mirror OpenSSH's `-L`/`-R` argument grammar so the mental model transfers directly. A bare port (`"5432"`) binds the loopback interface — never `0.0.0.0` — unless an explicit bind address is given (see [Security model](#security-model)). All structs keep `#[serde(deny_unknown_fields)]` per [03-configuration.md](../03-configuration.md). Per-template override is out of scope for the first cut (consistent with the "`ssh` is a top-level singleton" decision in [06](../06-launch-and-connect.md)).

---

## Proposed CLI surface

On both `kleya connect` and `kleya launch` (the latter only meaningful with `--connect`):

| Flag | Meaning |
|---|---|
| `-L <[bind:]lport:rhost:rport>` | Ad-hoc local forward, repeatable; passed straight through to `ssh -L`. |
| `-R <[bind:]rport:lhost:lport>` | Ad-hoc remote forward, repeatable; passed straight through to `ssh -R`. |
| `--forward-path <local:remote>` | Ad-hoc directory sync, repeatable. |
| `--print` | (existing) The rendered plan shows every `-L`/`-R` and path sync it would set up. |

Ad-hoc flags are **additive** over `[ssh.forward]` config. `-L`/`-R` use the exact OpenSSH spelling so muscle memory and copy-paste from `ssh` docs work unchanged.

---

## Argv and execution

`-L`/`-R` are appended to the argv built in [06-launch-and-connect.md](../06-launch-and-connect.md) (after the `-o` options, before the `-t … user@endpoint` tail). Because kleya `exec`s `ssh`, the forwards live and die with that process — no supervisor, matching native `ssh` semantics.

Directory sync runs **before** the interactive `exec`:

- `rsync-push`: `rsync -az -e "ssh -i <key> -o …" <local>/ <user>@<endpoint>:<remote>/`, reusing the same key and host-key policy as the connect. A non-zero rsync exit aborts the connect with a clear error rather than silently dropping into the shell.
- `sshfs` (future): mount the operator's local dir on the box via an `-R`-tunneled sshfs; requires `sshfs` on the box (bootstrap install) and is out of the first cut.

---

## Bootstrap changes needed

[05-bootstrap-rendering.md](../05-bootstrap-rendering.md) / `setup_devbox.sh.j2`:

1. Ensure `rsync` is installed for `--forward-path` / `[[ssh.forward.path]]` (`mode = "rsync-push"`).
2. (`sshfs` mode, future) install `sshfs` and ensure `user_allow_other` if a shared mount is needed.

No `sshd_config` changes are required for `-L`/`-R`: `AllowTcpForwarding` and `AllowStreamLocalForwarding` default to `yes` on the box's stock sshd.

---

## Security model

- **Loopback by default.** A bare-port `listen` binds `127.0.0.1`, never `0.0.0.0`. Binding a non-loopback address is possible but must be explicit in `listen`, and `-R` to a public bind additionally needs `GatewayPorts` on the box (off by default; kleya does not enable it implicitly).
- **`-R` exposes the laptop to the box.** A remote forward lets anything on the box reach the forwarded local service. Scope it to loopback targets and treat it like the credential forwards — the box is operator-owned and short-lived, but the forward is live for the session.
- **Validation before exec.** Malformed `listen`/`connect`/path specs fail at plan time (exit code 2, `ConfigInvalid`) rather than producing an opaque `ssh` usage error after the process is replaced.

---

## Decisions

- *`-L`/`-R` use OpenSSH spelling verbatim.* **Lowest-surprise surface; copy-paste from `ssh` docs and existing habits transfer.** Kleya validates and passes through rather than inventing a new grammar.
- *Loopback bind by default.* **A bare port must never silently listen on all interfaces** — the default dev box sits on a public subnet ([06](../06-launch-and-connect.md)).
- *No reconnect supervisor.* **Forwards share the lifetime of the `exec`d `ssh`, matching native semantics;** kleya does not add a daemon to keep them alive across drops.
- *Directory "forwarding" is rsync-push first.* **No native SSH primitive exists for a live mirror; `sshfs` is deferred** to avoid the dependency and its failure modes in the first cut.

## Open questions

- *Persisted `-R` for the credential-helper proxy.* The [credential-forwarding](2026-05-21-credential-forwarding.md) change wants a standard `-R` unix socket for no-disk HTTPS git. Should that socket be a first-class named entry here, or owned entirely by the credential change?
- *`autossh`-style resilience.* Long agentic sessions outlive flaky networks; native `ssh` forwards die on disconnect. Worth an opt-in keepalive/reconnect wrapper, or explicitly out of scope?
- *Live directory sync.* Is `sshfs` reverse-mount actually wanted, or is `rsync-push` plus the operator's own watch tooling enough?
- *Per-template forwards.* Deferred with the existing "`ssh` is a singleton" decision; revisit if a "db box" vs. "code box" split emerges.

---

## Canonical merge targets

On implementation, fold this document into:

- [06-launch-and-connect.md](../06-launch-and-connect.md) — argv assembly (`-L`/`-R` placement), the pre-exec rsync step, and forward resolution in `ConnectService::plan`.
- [03-configuration.md](../03-configuration.md) — the `[ssh.forward]` block, defaults table, and `canonical-types.schema.json` new `SshForwardCfg` / `PortForward` / `PathForward` defs.
- [02-cli-surface.md](../02-cli-surface.md) — `-L` / `-R` / `--forward-path` flags on `connect` and `launch`.
- [05-bootstrap-rendering.md](../05-bootstrap-rendering.md) — `rsync` install (and `sshfs`, when that mode lands).
