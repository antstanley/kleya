# Change — Credential forwarding

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley

A change spec proposing that `kleya connect` (and the `--connect` arm of `kleya launch`) be able to **forward or inject the operator's local credentials into the remote dev box**, so an agentic coding session there can `git push`, use `gh`, drive Claude Code, and reach the operator's cloud accounts without the operator hand-copying secrets.

Sibling change: [2026-05-21-ssh-port-forwarding.md](2026-05-21-ssh-port-forwarding.md) covers generic port/path forwarding. The two share the `[ssh]` config surface and the `ConnectService::plan` build path, and the no-disk HTTPS-git path here *consumes* a remote forward defined there — but they are otherwise independent.

This file is a staging document. Once the work lands it merges into the canonical specs listed under [Canonical merge targets](#canonical-merge-targets) and is deleted.

---

## Motivation

A freshly bootstrapped box is credential-blank. The operator lands in tmux and immediately hits friction: `git push` over SSH has no key, `gh` is unauthenticated, `claude` has no token, `aws`/`gcloud` calls fail. They work around it by pasting secrets into the remote shell — exactly the footgun [11-credentials-and-sso.md](../11-credentials-and-sso.md) tells operators to avoid for kleya's *own* provisioning creds.

Kleya is uniquely placed to fix this cleanly because it owns **both ends**: it renders the box's bootstrap *and* constructs the `ssh` invocation at connect time.

This is the **inverse** of [11-credentials-and-sso.md](../11-credentials-and-sso.md): that page is about credentials kleya consumes to *provision*; this is about credentials kleya relays *into* the box for the operator's session. The two must not be conflated, and this feature must not become a kleya-side credential store (see [Decisions](#decisions)).

---

## Goals

Ship **named credential presets** that "just work":

- `git` — local git credentials.
- `github` — `gh` CLI auth (and HTTPS git via `gh`).
- `agent` — Claude Code (agent) credentials.
- `aws` / `gcp` / `azure` / `cloudflare` — cloud-provider credentials.

Non-goal: persisting any of these in kleya config or a kleya-managed store. Everything is materialized transiently at connect time.

---

## Feasibility investigation

Probed a representative operator machine (macOS, OpenSSH 10.2). Findings drive the mechanism choice per preset — the recurring lesson is that **most modern creds are not plain files**; they live in an OS keychain or behind a CLI, so "copy the file" rarely works and a CLI/agent path is needed.

| Preset | Where the secret actually lives (observed) | Plain file to copy? | Recommended mechanism |
|---|---|---|---|
| `git` (SSH remotes) | `ssh-agent` (`SSH_AUTH_SOCK` live, ed25519 key loaded) | n/a | **Agent forwarding** (`ForwardAgent`) — zero secret on disk |
| `git` (HTTPS remotes) | macOS Keychain via `credential.helper=osxkeychain` | **No** | Credential-helper proxy over an `-R` socket (no disk), *or* materialize via `git credential fill` → token (on disk) |
| `git` (identity) | `~/.gitconfig` (`user.name`/`user.email`) | Yes | Copy `~/.gitconfig` (non-secret) |
| `github` | `~/.config/gh/hosts.yml` (0600) or keyring; token via `gh auth token` | Partially | `gh auth token` → inject `GH_TOKEN` |
| `agent` (Claude Code) | OS keychain; `~/.claude.json` is **config/state, not the secret** | **No** | Operator-generated long-lived `CLAUDE_CODE_OAUTH_TOKEN` (`claude setup-token`) or `ANTHROPIC_API_KEY` → inject env |
| `aws` | `~/.aws/config` + SSO/`login` cache (no static `credentials` file) | **No** (SSO/login) | `aws configure export-credentials --format env` → inject env (short TTL!) |
| `gcp` | `~/.config/gcloud/application_default_credentials.json` | Sometimes | Copy ADC file, *or* `gcloud auth ... print-access-token` → env |
| `azure` | `~/.azure/` token cache | Sometimes | Copy `~/.azure`, *or* `az account get-access-token` → env |
| `cloudflare` | `CLOUDFLARE_API_TOKEN` env / `~/.cloudflared/` | Sometimes | Inject `CLOUDFLARE_API_TOKEN` env, *or* copy `~/.cloudflared` |

**Verdict: all named presets are feasible**, realized through the two primitives below (plus, for the no-disk HTTPS-git path, a remote forward borrowed from the [port-forwarding](2026-05-21-ssh-port-forwarding.md) change).

### Injection primitives

1. **Native SSH agent forwarding** — `ForwardAgent yes`. Forwards `SSH_AUTH_SOCK`. Best path for git-over-SSH and onward SSH. **Nothing touches the remote disk.** Cost: anyone with root on the box can use the agent *for the session's duration*. Acceptable for an operator-owned, short-lived box; called out in [Security model](#security-model).
2. **Token materialization + injection** — for secrets that live in keychains/CLIs. Materialize locally at connect time (`gh auth token`, `aws configure export-credentials`, …) and inject into the remote session by **one of**:
   - **env-file (default):** write `~/.config/kleya/forward.env` on the box (mode `0600`) over the ssh stdin channel; bootstrap appends `[ -r ~/.config/kleya/forward.env ] && source ~/.config/kleya/forward.env` to `~/.zshrc`. Survives tmux reattach. Secret rests on disk on the (short-lived) box.
   - **env-only (`delivery = "env-only"`):** pass via `ssh -o SetEnv` (requires kleya to add matching `AcceptEnv` lines to the box's `sshd_config` in bootstrap). No disk, but does **not** survive tmux reattach because the value only decorates the initial exec.

> Implementation note on `SetEnv`: arbitrary env vars are gated by the remote `sshd` `AcceptEnv` allow-list (only `TERM` is special-cased — see [06-launch-and-connect.md](../06-launch-and-connect.md) `ssh.term`). The env-only mode therefore requires a bootstrap `AcceptEnv` edit; the env-file mode does not, which is why env-file is the default.

---

## Proposed config surface

New `[ssh.credentials]` block. Presets are booleans; **all default `false`** — no credential leaves the operator's machine unless explicitly opted in, per invocation or in config.

```toml
[ssh.credentials]
git        = false    # agent-forward + ~/.gitconfig; enable with `git = true` or `--creds git`
github     = false    # gh auth token  -> GH_TOKEN
agent      = false    # Claude Code token (CLAUDE_CODE_OAUTH_TOKEN / ANTHROPIC_API_KEY)
aws        = false    # aws configure export-credentials -> AWS_* env
gcp        = false
azure      = false
cloudflare = false

# How materialized tokens are delivered: "env-file" (persists, on disk, 0600)
# or "env-only" (no disk, lost on tmux reattach). Default env-file.
delivery   = "env-file"
```

All structs keep `#[serde(deny_unknown_fields)]` per [03-configuration.md](../03-configuration.md). Per-template override is out of scope for the first cut (consistent with the "`ssh` is a top-level singleton" decision in [06](../06-launch-and-connect.md)).

---

## Proposed CLI surface

On both `kleya connect` and `kleya launch` (the latter only meaningful with `--connect`):

| Flag | Meaning |
|---|---|
| `--creds <preset[,preset…]>` | Enable named presets for this invocation (e.g. `--creds github,aws`). Additive over config. |
| `--no-creds <preset[,preset…]>` | Disable presets for this invocation (e.g. `--no-creds git` to suppress the default). |
| `--print` | (existing) The rendered plan must show which credentials would be materialized — **without printing the secret values**. |

Resolution precedence mirrors [03-configuration.md](../03-configuration.md): CLI flags override `[ssh.credentials]` config, which overrides built-in defaults (all presets off).

---

## Named presets — exact realization

### `git`

1. `ForwardAgent yes` added to the argv — covers SSH remotes and onward SSH.
2. Copy `~/.gitconfig` `user.name` / `user.email` (and nothing secret) into the box's `~/.gitconfig` so commits are attributed correctly.
3. HTTPS remotes: when `github` is also on, `gh auth setup-git` on the box wires git→`gh` (uses `GH_TOKEN`). Otherwise, the credential-helper proxy over an `-R` socket is the no-disk option (deferred; depends on the [port-forwarding](2026-05-21-ssh-port-forwarding.md) change — see Open questions).

Enable with `git = true` or `--creds git`; it stays the most benign preset since it is agent-forwarding-first (no secret on disk).

### `github`

`gh auth token` locally → inject `GH_TOKEN` via the chosen delivery. The box already installs `gh` (see [05-bootstrap-rendering.md](../05-bootstrap-rendering.md)); `GH_TOKEN` authenticates both `gh` and, via `gh auth setup-git`, HTTPS git.

### `agent` (Claude Code)

Inject `CLAUDE_CODE_OAUTH_TOKEN` (preferred — generated by the operator via `claude setup-token`, intended for headless use) or `ANTHROPIC_API_KEY`. The box already installs Claude Code. **Do not** attempt to scrape the operator's interactive subscription credentials out of the OS keychain — surfaced as an explicit operator step, with a ToS caveat in [Security model](#security-model).

### `aws` / `gcp` / `azure` / `cloudflare`

Default mechanism is **materialize-to-env**, because the observed local state for all four is keychain/SSO/CLI-backed rather than a portable file:

- `aws` → `aws configure export-credentials --format env` (honors SSO/`login`/role chains; yields *temporary* `AWS_ACCESS_KEY_ID`/`_SECRET_/_SESSION_TOKEN` with a TTL — see Open questions on refresh).
- `gcp` → copy ADC json, or `gcloud auth application-default print-access-token` → `GOOGLE_*`.
- `azure` → copy `~/.azure`, or `az account get-access-token`.
- `cloudflare` → `CLOUDFLARE_API_TOKEN` env, or copy `~/.cloudflared`.

For AWS specifically there is a cleaner long-term alternative: kleya attaches an **instance profile / IAM role** to the box at launch, so the box gets creds from IMDS and nothing is forwarded at all. Tracked as an Open question; the export-credentials path is the immediate, provider-uniform answer.

---

## Bootstrap changes needed

[05-bootstrap-rendering.md](../05-bootstrap-rendering.md) / `setup_devbox.sh.j2`:

1. Append the `forward.env` source line to `~/.zshrc` (guarded; no-op when the file is absent), gated on a new `enable_credential_forwarding` render var so a box can opt out entirely.
2. (env-only mode only) add `AcceptEnv` entries to `sshd_config` for the injected variable names and reload sshd.

Connect-time (not bootstrap): kleya writes `~/.config/kleya/forward.env` over the ssh control channel at mode `0600`, materializing each enabled preset's token immediately before the `exec ssh`.

---

## Security model

Forwarding a credential to a box **grants that box — and anyone with root on it — use of that credential**. Mitigations, in priority order:

1. **Prefer no-disk mechanisms.** Agent forwarding and the credential-helper proxy never write a secret to the box. Token materialization is a fallback for creds that have no socket form.
2. **Short-lived, operator-owned boxes.** Kleya boxes are disposable; the blast radius is the session lifetime. `terminate` destroys the disk that any `forward.env` lived on.
3. **Off by default, opt-in per preset.** No preset forwards anything unless the operator explicitly enables it (config or `--creds`). Nothing is implicit.
4. **Least privilege at the source.** Recommend scoped tokens: `claude setup-token` (not subscription creds), fine-grained `gh`/PATs, short-TTL `aws` session creds, scoped Cloudflare API tokens.
5. **`0600` + cleanup.** `forward.env` is mode `0600`, owned by the login user; removed on `terminate` with the box.

**Caveats to document for operators:**
- Forwarding a personal Claude *subscription* into a shared/cloud box may violate Anthropic ToS — use `setup-token`/API-key paths.
- Materialized AWS creds are temporary and **expire mid-session**; re-running `kleya connect` re-materializes them.
- Agent forwarding to an *untrusted* box is unsafe; kleya assumes the box is operator-owned.

---

## Decisions

- *This is not a credential store.* **Everything is materialized transiently at connect time and either forwarded over a socket or written to a `0600` file that dies with the box.** Aligns with [11-credentials-and-sso.md](../11-credentials-and-sso.md)'s "no kleya-side credentials store" rule — kleya reads from where the secret already lives (agent, keychain, CLI) and never persists a copy in kleya config.
- *All presets off by default — secure by default.* **No credential leaves the operator's machine without an explicit opt-in** (`--creds <preset>` or config). `git`, being agent-forwarding-first (no secret on disk), is the safest to enable but is still off until requested.
- *`env-file` is the default delivery, not `env-only`.* **tmux is the default session model ([06](../06-launch-and-connect.md)); env-only values are lost on reattach.** Operators who want zero-disk accept the reattach limitation via `delivery = "env-only"`.

## Open questions

- *Credential-helper proxy for HTTPS git.* The no-disk path for `osxkeychain`-style git creds needs a small remote helper that proxies `git credential` back over an `-R` socket (defined by the [port-forwarding](2026-05-21-ssh-port-forwarding.md) change). Worth it, but more implementation than `gh`-based HTTPS. Ship `gh`-backed HTTPS first?
- *AWS via instance profile vs. forwarding.* Attaching an IAM role at launch (kleya gains an `iam:PassRole` requirement and an instance-profile config field) removes AWS forwarding entirely and never expires mid-session. Is that the preferred AWS story, with export-credentials as the fallback?
- *Token refresh.* Materialized SSO/short-TTL creds expire during long sessions. Re-running `connect` re-materializes, but is an on-box refresh helper wanted?
- *Windows / non-macOS keychains.* Feasibility was probed on macOS. Linux operators (Secret Service / `pass`, file-based `~/.aws/credentials`) shift several rows of the feasibility table toward "plain file" — confirm before documenting as cross-platform.

---

## Canonical merge targets

On implementation, fold this document into:

- [06-launch-and-connect.md](../06-launch-and-connect.md) — argv assembly (`ForwardAgent`), connect-time `forward.env` write, the credential-resolution step in `ConnectService::plan`.
- [03-configuration.md](../03-configuration.md) — the `[ssh.credentials]` block, defaults table, and `canonical-types.schema.json` new `SshCredentialsCfg` def.
- [02-cli-surface.md](../02-cli-surface.md) — `--creds` / `--no-creds` flags on `connect` and `launch`.
- [05-bootstrap-rendering.md](../05-bootstrap-rendering.md) — `~/.zshrc` source line, optional `AcceptEnv`, the `enable_credential_forwarding` render var.
- [11-credentials-and-sso.md](../11-credentials-and-sso.md) — a new "Forwarding operator credentials into the box" section explicitly contrasting it with kleya's own provisioning-credential chain.
