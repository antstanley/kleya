# Change: Credential forwarding

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley · **Target:** Repo-wide

`kleya connect` (and the `--connect` arm of `kleya launch`) will be able to **forward or inject the operator's local credentials into the remote dev box**, so an agentic coding session there can `git push`, use `gh`, drive Claude Code, and reach the operator's cloud accounts without the operator hand-copying secrets. Credentials are shipped as **named presets** (`git`, `github`, `agent`, `aws`, `gcp`, `azure`, `cloudflare`), all **off by default**, materialised transiently at connect time and either agent-forwarded over a socket or written to a `0600` env-file that dies with the box. This is **not** a kleya-side credential store.

Sibling changes: [2026-05-21-ssh_port_forwarding.md](2026-05-21-ssh_port_forwarding.md) covers generic port/path forwarding (the no-disk HTTPS-git path here consumes a remote forward defined there), and [2026-05-21-env_var_forwarding.md](2026-05-21-env_var_forwarding.md) forwards arbitrary environment variables and **shares the same `~/.config/kleya/forward.env` writer** defined here.

---

## Motivation

A freshly bootstrapped box is credential-blank. The operator lands in tmux and immediately hits friction: `git push` over SSH has no key, `gh` is unauthenticated, `claude` has no token, `aws`/`gcloud` calls fail. They work around it by pasting secrets into the remote shell — exactly the footgun [11-credentials-and-sso.md](../11-credentials-and-sso.md) tells operators to avoid for kleya's *own* provisioning creds. Kleya is uniquely placed to fix this cleanly because it owns **both ends**: it renders the box's bootstrap *and* constructs the `ssh` invocation at connect time.

This is the **inverse** of [11-credentials-and-sso.md](../11-credentials-and-sso.md): that page is about credentials kleya consumes to *provision*; this is about credentials kleya relays *into* the box for the operator's session. The two must not be conflated, and this feature must not become a kleya-side credential store (see [Decisions](#assumptions-and-open-questions)). The non-goal is explicit: nothing is persisted in kleya config or a kleya-managed store; everything is materialised transiently at connect time.

---

## Affected spec pages

| Canonical page | Nature of change |
|---|---|
| [`docs/specs/03-configuration.md`](../03-configuration.md) | Add `[ssh.credentials]` block (preset booleans + `delivery`), defaults rows |
| [`docs/specs/06-launch-and-connect.md`](../06-launch-and-connect.md) | Add `ForwardAgent` argv option, connect-time `forward.env` write, credential resolution in `ConnectService::plan`, and the `--print` redaction rule under `env-only` |
| [`docs/specs/02-cli-surface.md`](../02-cli-surface.md) | Add `--creds` / `--no-creds` flags to `connect` and `launch` |
| [`docs/specs/05-bootstrap-rendering.md`](../05-bootstrap-rendering.md) | Add the `~/.zshrc` `forward.env` source line under an `enable_credential_forwarding` render var; optional `AcceptEnv` for `env-only` |
| [`docs/specs/11-credentials-and-sso.md`](../11-credentials-and-sso.md) | Add a "Forwarding operator credentials into the box" section contrasting it with kleya's own provisioning chain |
| [`canonical-types.schema.json`](../canonical-types.schema.json) | Add `SshCredentialsCfg` `$def`; add optional `credentials` to `SshCfg` |

---

## Feasibility (investigation summary)

Probed a representative operator machine (macOS, OpenSSH 10.2). The recurring lesson: **most modern creds are not plain files** — they live in an OS keychain or behind a CLI, so "copy the file" rarely works and a CLI/agent path is needed. This drives the per-preset mechanism choice in the `Proposed changes` blocks below.

| Preset | Where the secret actually lives (observed) | Plain file? | Mechanism |
|---|---|---|---|
| `git` (SSH remotes) | `ssh-agent` (`SSH_AUTH_SOCK` live, ed25519 loaded) | n/a | Agent forwarding (`ForwardAgent`) — zero secret on disk |
| `git` (HTTPS remotes) | macOS Keychain via `credential.helper=osxkeychain` | No | `gh`-backed HTTPS, or a credential-helper proxy over an `-R` socket (no disk) |
| `git` (identity) | `~/.gitconfig` (`user.name`/`user.email`) | Yes | Copy `~/.gitconfig` (non-secret) |
| `github` | `~/.config/gh/hosts.yml` (0600) or keyring | Partially | `gh auth token` → inject `GH_TOKEN` |
| `agent` (Claude Code) | OS keychain; `~/.claude.json` is config/state, not the secret | No | Operator-generated `CLAUDE_CODE_OAUTH_TOKEN` (`claude setup-token`) or `ANTHROPIC_API_KEY` → env |
| `aws` | `~/.aws/config` + SSO/`login` cache (no static `credentials`) | No (SSO) | `aws configure export-credentials --format env` → env (short TTL) |
| `gcp` | `~/.config/gcloud/application_default_credentials.json` | Sometimes | Copy ADC file, or `gcloud auth ... print-access-token` → env |
| `azure` | `~/.azure/` token cache | Sometimes | Copy `~/.azure`, or `az account get-access-token` → env |
| `cloudflare` | `CLOUDFLARE_API_TOKEN` env / `~/.cloudflared/` | Sometimes | Inject `CLOUDFLARE_API_TOKEN`, or copy `~/.cloudflared` |

All presets are feasible via two primitives: **(1) native SSH agent forwarding** (`ForwardAgent yes`, nothing on the remote disk) and **(2) token materialisation + injection** for secrets that live in keychains/CLIs, delivered either as an env-file (`~/.config/kleya/forward.env`, default) or via `ssh -o SetEnv` (`env-only`, no disk but lost on tmux reattach and gated by `AcceptEnv`).

---

## Proposed changes

One subsection per affected page. Each block is the prose as it should read once merged.

### `docs/specs/03-configuration.md` → Canonical schema (Modify)

Add a `[ssh.credentials]` block to the canonical TOML, after `[ssh].extra_args`:

> ```toml
> # Operator credentials forwarded into the box. All presets default false:
> # no credential leaves the operator's machine unless explicitly opted in.
> [ssh.credentials]
> git        = false   # agent-forward + ~/.gitconfig identity
> github     = false   # gh auth token  -> GH_TOKEN
> agent      = false   # Claude Code token (CLAUDE_CODE_OAUTH_TOKEN / ANTHROPIC_API_KEY)
> aws        = false   # aws configure export-credentials -> AWS_* env (short TTL)
> gcp        = false
> azure      = false
> cloudflare = false
>
> # How materialised tokens reach the box:
> #   "env-file" (default) — written to ~/.config/kleya/forward.env at 0600; survives tmux reattach.
> #   "env-only"           — passed via `ssh -o SetEnv`; no disk, lost on reattach, needs AcceptEnv.
> delivery   = "env-file"
> ```
>
> `ssh.credentials` is optional; an absent block forwards nothing. All structs keep `#[serde(deny_unknown_fields)]`. Resolution precedence matches the rest of `03`: CLI flags (`--creds`/`--no-creds`) override `[ssh.credentials]` config, which overrides the built-in defaults (all presets off).

### `docs/specs/06-launch-and-connect.md` → Argv assembly (Modify)

> When the `git` preset is enabled, `ForwardAgent yes` is added to the argv (covering git-over-SSH and onward SSH); nothing touches the remote disk. Materialised-token presets (`github`, `agent`, `aws`, …) do **not** change the argv under the default `env-file` delivery — their values go over the ssh channel into `~/.config/kleya/forward.env`. Under `delivery = "env-only"` each token becomes an `-o "SetEnv NAME=value"` argv element in the same block as the existing `SetEnv TERM=…`.

### `docs/specs/06-launch-and-connect.md` → `ConnectService::plan` orchestration (Modify)

> A credential-resolution step runs after key resolution and before the interactive `exec`. For each enabled preset, kleya materialises the token locally at connect time (`gh auth token`, `aws configure export-credentials --format env`, …) and, under `env-file` delivery, writes the assembled `export` lines to `~/.config/kleya/forward.env` on the box over the ssh channel at mode `0600`. The `git` preset additionally copies `~/.gitconfig` `user.name`/`user.email` (non-secret) so commits are attributed; HTTPS git is wired via `gh auth setup-git` when `github` is also enabled.
>
> **Single `forward.env` writer.** The [env-var-forwarding](2026-05-21-env_var_forwarding.md) change writes the same file. There is one writer that assembles credential `export` lines and env-var `export` lines, then writes the file once per connect, truncating any prior copy.

### `docs/specs/06-launch-and-connect.md` → CLI `Cmd::Connect` arm / `--print` (Modify)

> Under `env-only` delivery, materialised tokens become `-o "SetEnv NAME=value"` argv elements, so a verbatim `plan.argv` dump would leak them. `--print` therefore redacts `SetEnv` values (showing `SetEnv NAME=<materialized>`) rather than echoing argv. Under the default `env-file` delivery the values never enter argv, so `--print` is unaffected. (The [env-var-forwarding](2026-05-21-env_var_forwarding.md) change documents the identical `--print` contract change for its `set-env` delivery.)

### `docs/specs/02-cli-surface.md` → `kleya connect` / `kleya launch` (Add)

> | Flag | Effect |
> |---|---|
> | `--creds <preset[,preset…]>` | Enable named presets for this invocation (e.g. `--creds github,aws`). Additive over `[ssh.credentials]` config |
> | `--no-creds <preset[,preset…]>` | Disable presets for this invocation (e.g. `--no-creds git`) |
>
> Presets are `git`, `github`, `agent`, `aws`, `gcp`, `azure`, `cloudflare`. `--print` shows which credentials would be materialised **without** printing the secret values; under `env-only` delivery it redacts the `SetEnv` values (see [06](../06-launch-and-connect.md)).

### `docs/specs/05-bootstrap-rendering.md` → Template variables / body (Modify)

> A new `enable_credential_forwarding` render var (default `false`) gates a guarded line appended to `~/.zshrc`:
> ```sh
> [ -r ~/.config/kleya/forward.env ] && source ~/.config/kleya/forward.env
> ```
> The line is a no-op when the file is absent, and re-sources on every new shell so a reattached tmux pane still sees the values. The connect-time write of `~/.config/kleya/forward.env` (mode `0600`) is not a bootstrap step — kleya writes it over the ssh channel at connect. Under `env-only` delivery the bootstrap additionally adds `AcceptEnv` entries to `sshd_config` for the injected variable names and reloads `sshd`; this is why `env-file` (no `sshd_config` change) is the default. (The env-var-forwarding change reuses the same source line under a collapsed `enable_*_forwarding` toggle.)

### `docs/specs/11-credentials-and-sso.md` → new section "Forwarding operator credentials into the box" (Add)

> Distinct from the provisioning-credential chain above: kleya can relay the operator's *own* credentials into a launched box for the interactive/agentic session (the `[ssh.credentials]` presets). This is the inverse direction — credentials kleya pushes *into* the box, not credentials it consumes to provision. It is **not** a credential store: kleya reads from where the secret already lives (ssh-agent, keychain, CLI), materialises transiently at connect time, and never persists a copy. Forwarding a credential grants the box — and anyone with root on it — use of that credential for the session's lifetime; mitigations are off-by-default presets, no-disk mechanisms first, scoped/short-lived tokens, and `0600` files that die with the box on `terminate`.

---

## Type changes

Fragment for the `[ssh.credentials]` config. Folds into `canonical-types.schema.json` on merge. `credentials` is added to `SshCfg.properties` but **not** to its `required` array, so existing configs keep validating and the round-trip property test in `03` still holds.

```json
{
  "$comment": "Fragment for 2026-05-21-credential_forwarding. Folds into canonical-types.schema.json on merge.",
  "$defs": {
    "SshCredentialsCfg": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "git":        { "type": "boolean", "default": false },
        "github":     { "type": "boolean", "default": false },
        "agent":      { "type": "boolean", "default": false },
        "aws":        { "type": "boolean", "default": false },
        "gcp":        { "type": "boolean", "default": false },
        "azure":      { "type": "boolean", "default": false },
        "cloudflare": { "type": "boolean", "default": false },
        "delivery":   { "type": "string", "enum": ["env-file", "env-only"], "default": "env-file" }
      }
    },
    "SshCfg": {
      "$comment": "Modified: add optional `credentials`. Shown with the new property only; merge into the existing SshCfg def, leaving `required` unchanged.",
      "properties": {
        "credentials": { "$ref": "#/$defs/SshCredentialsCfg" }
      }
    }
  }
}
```

---

## Implementation notes

Pointers for the implementing agent. The `forward.env` writer is shared with the env-var-forwarding change — build it once.

```
1. Add SshCredentialsCfg to crates/kleya-core/src/config.rs, with
   `credentials: Option<SshCredentialsCfg>` on SshCfg (serde default None).
2. Add a credential-materialisation module in kleya-core: per preset, shell out to the
   local CLI (`gh auth token`, `aws configure export-credentials --format env`, …) and
   collect NAME=value pairs. Never log values.
3. Add the forward.env writer (shared with env-var forwarding): assemble export lines,
   write once over the ssh channel at 0600. Order credential lines before env-var lines.
4. Add ForwardAgent to build_argv in crates/kleya-core/src/commands/connect.rs:140 when the
   git preset is on; gate SetEnv injection on delivery=="env-only".
5. Change the Cmd::Connect --print path (crates/kleya-cli) to redact SetEnv values rather
   than dumping plan.argv verbatim — see 06.
6. Add --creds/--no-creds to crates/kleya-cli/src/clap_args.rs (comma lists, additive over config).
7. Add the enable_credential_forwarding render var + ~/.zshrc source line (and optional
   AcceptEnv) to the setup_devbox.sh.j2 template (crates/kleya-core/src/bootstrap/render.rs:37
   BootstrapVars).
```

References: `gh auth token`, `gh auth setup-git`; `aws configure export-credentials` (AWS CLI v2); `claude setup-token` (Claude Code headless OAuth token); OpenSSH `ForwardAgent`, `SetEnv`/`AcceptEnv`.

---

## Merge plan

1. Apply each `Proposed changes` block to its canonical page; bump each touched page's `**Date:**` to the merge date. The `11` block is a new section, not a modification.
2. Fold the `Type changes` `$defs` into `canonical-types.schema.json` (`SshCredentialsCfg`, and the `credentials` property on `SshCfg` — leave `SshCfg.required` unchanged).
3. If the env-var-forwarding change has not landed, this change owns the `forward.env` writer and the `enable_credential_forwarding` render var outright; if it has, reconcile to the single shared writer and the collapsed `enable_*_forwarding` toggle.
4. Flip this file's `**Status:**` to `Merged`, add `**Merged:** YYYY-MM-DD`, and move it to `docs/specs/changes/merged/`.
5. Update `docs/README.md`: remove this file from the pending Change-specs list.

---

## Assumptions and open questions

**Assumptions**

- The box is **operator-owned and short-lived**; the blast radius of any forwarded credential is the session lifetime, and `terminate` destroys the disk any `forward.env` lived on.
- The operator's local CLIs (`gh`, `aws`, `gcloud`, `az`) are installed and authenticated, so the materialisation commands succeed; a failed materialisation aborts with a clear error rather than forwarding nothing silently.
- Feasibility was probed on macOS (OpenSSH 10.2). Linux operators (Secret Service / `pass`, file-based `~/.aws/credentials`) shift several feasibility rows toward "plain file"; cross-platform behaviour is an open question below.
- Agent forwarding to the box is acceptable because the box is operator-owned; forwarding to an untrusted box would not be.

**Decisions**

- *This is not a credential store.* **Everything is materialised transiently at connect time and either forwarded over a socket or written to a `0600` file that dies with the box.** Aligns with [11-credentials-and-sso.md](../11-credentials-and-sso.md)'s "no kleya-side credentials store" rule — kleya reads from where the secret already lives and never persists a copy.
- *All presets off by default — secure by default.* **No credential leaves the operator's machine without an explicit opt-in** (`--creds <preset>` or config). `git`, being agent-forwarding-first (no secret on disk), is the safest to enable but is still off until requested.
- *`env-file` is the default delivery, not `env-only`.* **tmux is the default session model ([06](../06-launch-and-connect.md)); `env-only` values are lost on reattach.** Operators who want zero-disk accept the reattach limitation via `delivery = "env-only"`.
- *Prefer no-disk mechanisms; least privilege at the source.* **Agent forwarding and the credential-helper proxy never write a secret to the box; token materialisation is the fallback for creds with no socket form.** Recommend scoped tokens: `claude setup-token` (not subscription creds), fine-grained `gh`/PATs, short-TTL `aws` session creds, scoped Cloudflare tokens.

**Operator caveats to document**

- Forwarding a personal Claude *subscription* into a shared/cloud box may violate Anthropic ToS — use `setup-token`/API-key paths.
- Materialised AWS creds are temporary and expire mid-session; re-running `kleya connect` re-materialises them.

**Open questions**

- *Credential-helper proxy for HTTPS git.* The no-disk path for `osxkeychain`-style git creds needs a small remote helper that proxies `git credential` back over an `-R` socket (defined by the [port-forwarding](2026-05-21-ssh_port_forwarding.md) change). Worth it, but more work than `gh`-based HTTPS. Ship `gh`-backed HTTPS first?
- *AWS via instance profile vs. forwarding.* Attaching an IAM role at launch (kleya gains an `iam:PassRole` requirement and an instance-profile config field) removes AWS forwarding entirely and never expires mid-session. Is that the preferred AWS story, with export-credentials as the fallback?
- *Token refresh.* Materialised SSO/short-TTL creds expire during long sessions. Re-running `connect` re-materialises; is an on-box refresh helper wanted?
- *Windows / non-macOS keychains.* Feasibility was probed on macOS; confirm the Linux/Windows credential locations before documenting as cross-platform.
