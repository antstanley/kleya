# Change: Forwarding local environment variables into the box

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley · **Target:** Repo-wide

`kleya connect` (and the `--connect` arm of `kleya launch`) will be able to **share the operator's local environment variables with the remote dev box** — a named subset (by exact name or glob), explicit `NAME=value` pairs, or the whole environment — so an agentic session inherits the operator's `EDITOR`, feature flags, endpoint URLs, and other shell context without hand-`export`ing them on the box. The **default is none**: nothing crosses unless explicitly opted in. A non-overridable deny floor never lets host-identity variables (`PATH`, `HOME`, …) cross, and a default-on secret-name skip keeps the whole-environment sweep from leaking common tokens.

Sibling changes: [2026-05-21-credential_forwarding.md](2026-05-21-credential_forwarding.md) forwards the operator's *credentials* (named presets that materialise tokens), and [2026-05-21-ssh_port_forwarding.md](2026-05-21-ssh_port_forwarding.md) forwards *ports and paths*. This change is the generic **environment** layer — arbitrary `NAME=value` pairs — and it **shares the env-file writer** the credential change defines (`~/.config/kleya/forward.env`, sourced from `~/.zshrc`): the two write into the same file rather than inventing a second one.

---

## Motivation

The operator's local shell carries context the box does not: `EDITOR`/`VISUAL`, `LANG`, `HTTP_PROXY`, project feature flags (`MYAPP_ENV=staging`), service endpoints, model selectors, and the occasional API key. After bootstrap the box is environment-blank beyond what `~/.zshrc` sets, so the operator re-`export`s the same handful of vars by hand on every connect — and gets it subtly wrong when a value contains a space or a quote.

Kleya already builds the `ssh` argv ([06-launch-and-connect.md](../06-launch-and-connect.md)) and (per the credential change) writes an env-file over the ssh channel, so surfacing "forward these env vars" is a small addition on top of that machinery. The genuinely hard part is **getting values across intact** — values with spaces, single quotes, double quotes, `$`, backticks, or newlines must arrive byte-for-byte. This change is value-agnostic (it forwards whatever the operator names, with no knowledge of what the value means), which is what distinguishes it from credential forwarding. Non-goal: a kleya-side environment store, or secret detection/redaction of *values* — kleya cannot reliably tell a token from a flag.

---

## Affected spec pages

| Canonical page | Nature of change |
|---|---|
| [`docs/specs/03-configuration.md`](../03-configuration.md) | Add `[ssh.env]` block, the glob rule, the built-in deny floor, the secret default-deny set, and the per-template `[templates.env]` overlay + merge rules (resolving the per-template-override open question for `env` only) |
| [`docs/specs/06-launch-and-connect.md`](../06-launch-and-connect.md) | Add `-o SetEnv` placement (for `set-env`), the shared `forward.env` write, the `kleya:template`→template-`env` overlay lookup, env resolution in `ConnectService::plan`, and the `--print` redaction change |
| [`docs/specs/02-cli-surface.md`](../02-cli-surface.md) | Add `--env` / `--env-all` / `--forward-secrets` / `--no-env` flags, the two-tier resolution precedence, and the `--print` redaction rule |
| [`docs/specs/05-bootstrap-rendering.md`](../05-bootstrap-rendering.md) | Reuse the shared `forward.env` source line; optional `AcceptEnv` for `set-env`, under a merged `enable_*_forwarding` render var |
| [`canonical-types.schema.json`](../canonical-types.schema.json) | Add `SshEnvCfg` `$def`; add optional `env` to `SshCfg` and `TemplateCfg` |

---

## Feasibility (mechanism choice)

Two native SSH primitives can carry env vars; both have sharp edges that drive the design.

| Mechanism | Carries arbitrary values? | Needs box-side change? | Survives tmux reattach? |
|---|---|---|---|
| `ssh -o SetEnv NAME=value` | **No** — OpenSSH splits the option line on whitespace, so a value with a space/quote cannot be expressed reliably | **Yes** — gated by the box's `sshd` `AcceptEnv` allow-list (only `TERM` is special-cased; see [06](../06-launch-and-connect.md) `ssh.term`) | **No** — decorates only the initial `exec`'d pty |
| env-file sourced from `~/.zshrc` | **Yes** — each var is a shell-quoted `export` line | No (just the source line, added by the credential change) | **Yes** — re-sourced by every new shell, including reattached tmux panes |

**env-file is the default and the only mechanism that satisfies the deny-floor goal.** `set-env` is offered as an opt-in (`delivery = "set-env"`) for operators who want zero-disk and forward only space-free values from an **enumerable** set of names — but because `AcceptEnv` is fixed at bootstrap, `set-env` cannot carry `forward_all` or connect-resolved globs at all. The `SetEnv` ↔ `AcceptEnv` coupling is the same constraint documented for the credential change's `env-only` delivery.

---

## Proposed changes

One subsection per affected page. Each block is the prose as it should read once merged.

### `docs/specs/03-configuration.md` → Canonical schema (Modify)

Add an `[ssh.env]` block to the canonical TOML, after `[ssh].extra_args`. **All forwarding is off by default.**

> ```toml
> [ssh.env]
> forward      = []          # names/globs of local env vars to forward; default none
> forward_all  = false       # forward the operator's entire environment; default off
> deny         = []          # names/globs that must never be forwarded; wins over everything
> deny_secrets = true        # under forward_all, also skip the built-in secret-name globs (opt-out)
> delivery     = "env-file"  # "env-file" (default, on disk 0600, any value, survives reattach)
>                            #  | "set-env" (no disk, lost on reattach, space-free values, needs AcceptEnv)
> ```
>
> `forward` and `deny` are lists of variable **names or globs** (not `NAME=value`); a forwarded entry's value is read from the operator's environment at connect time, and a named-but-unset variable is skipped with a warning rather than forwarded empty. **Glob syntax** is shell-style `*` (zero-or-more characters) matched against the whole name — `MYAPP_*`, `*_TOKEN`, `*SECRET*`; a name with no `*` is an exact match; matching is case-sensitive and no other metacharacters are interpreted. `ssh.env` is optional. All structs keep `#[serde(deny_unknown_fields)]`.
>
> **Built-in deny floor.** Independently of config, kleya **always** denies host/shell-identity variables that would corrupt or mislead the remote shell: `PATH`, `HOME`, `SHELL`, `USER`, `LOGNAME`, `PWD`, `OLDPWD`, `SHLVL`, `TERM`, and the `LD_*` / `DYLD_*` families. This floor is **not overridable** by `forward`, `forward_all`, an explicit `--env PATH=…`, or by removing a `deny` entry. `PATH` is the canonical case: the box's own `PATH` (cargo, nvm, `~/.local/bin`; see [05-bootstrap-rendering.md](../05-bootstrap-rendering.md)) must never be clobbered.
>
> **Secret default-deny under `forward_all`.** Because sweeping the whole environment is the easiest way to leak a token, `forward_all` additionally skips a built-in set of secret-looking globs unless `deny_secrets = false`: `*TOKEN*`, `*SECRET*`, `*PASSWORD*`, `*PASSWD*`, `*_KEY`, `*APIKEY*`, `*ACCESS_KEY*`, `AWS_*`, `ANTHROPIC_*`, `OPENAI_*`. It is a safety net for the sweep only — it never blocks a variable named explicitly via `forward`/`--env`. It matches names, not values, so it is a heuristic, not a guarantee.

### `docs/specs/03-configuration.md` → Per-template overlay (Add) and the per-template open question (Resolve)

> A template may carry its own `env` overlay with the **same shape** as the global `[ssh.env]`, every field optional:
> ```toml
> [[templates]]
> name = "gpu"
> [templates.env]                              # same shape as [ssh.env]; all fields optional
> forward = ["CUDA_VISIBLE_DEVICES", "HF_*"]   # added to the global forward set
> deny    = ["MYAPP_*"]                        # added to the global deny set
> ```
> The overlay merges field-by-field over the global block: `forward` and `deny` **union** (a template can add forwards/denies, never *un-deny* a global or floor entry); `forward_all`, `deny_secrets`, `delivery` are **scalar overrides** (present in the template wins; absent inherits). The per-template struct uses `Option` scalars and empty-defaulting lists, so a template setting only `forward` leaves the other fields as the global block had them. The built-in floor and the secret default-deny apply regardless of template.
>
> This **resolves** the "Per-template `[ssh]` overrides" open question previously deferred on this page — narrowly, for `env` only. Environment is the one connect setting that is genuinely *box-shaped* (a GPU box wants `CUDA_*`, a staging-debug box wants different flags); the rest of `[ssh]` (user, tmux, term, extra_args, forwards, credentials) is operator-shaped and stays a top-level singleton.

### `docs/specs/06-launch-and-connect.md` → `ConnectService::plan` orchestration (Modify)

> An env-resolution step runs after the instance is resolved and before the interactive `exec`. The forwarded set is built once at plan time:
> ```
> 0. env        = global [ssh.env]  ⊕  template [templates.env]   (lists union, scalars override)
> 1. explicit   = env.forward globs ∪ --env names/globs ∪ explicit --env NAME=value
> 2. implicit   = entire environment            (only if env.forward_all || --env-all)
>    implicit  -= secret default-deny globs      (unless deny_secrets = false / --forward-secrets)
> 3. candidates = explicit ∪ implicit
> 4. hard_deny  = built-in floor ∪ env.deny globs ∪ --no-env names/globs
> 5. forwarded  = { c ∈ candidates : c matches no hard_deny entry }
> ```
> The template overlay is found via the `kleya:template` tag the launch adapter already stamps on the instance (the same tag bag `resolve_key` reads `kleya:key`/`kleya:managed` from) — no new tag, no state file. A missing `kleya:template` tag, or a tag naming a template absent from the loaded config, **warns and falls back to global-only** rather than failing the connect. The two deny strengths are deliberate: `hard_deny` strips any match regardless of how it entered `candidates` (including an explicit `--env PATH=/foo`); the secret default-deny only trims the implicit sweep and defers to explicit intent.

### `docs/specs/06-launch-and-connect.md` → Value escaping for `forward.env` (Add)

> Under `env-file` delivery, each forwarded value is emitted as a **single-quoted** shell `export` so the box reproduces it verbatim:
> ```
> export NAME='<value, each ' replaced by the four chars '\'' >'
> ```
> Algorithm for a value `v`: open `'`; for each character, if it is `'` emit `'\''` (close, escaped literal quote, reopen), else emit it unchanged; close `'`. Inside single quotes the shell interprets nothing, so spaces, double quotes, `$`, backticks, `\`, and embedded newlines all pass through. Variable **names** are validated against `^[A-Za-z_][A-Za-z0-9_]*$` before emission (a failing name is `ConfigInvalid` at plan time); validation applies to the **resolved** names actually forwarded — `forward`/`deny` glob patterns are matched against the environment first, so a `*` never reaches an emitted line. The file is the **same** `~/.config/kleya/forward.env` (mode `0600`) the [credential-forwarding](2026-05-21-credential_forwarding.md) change writes; one writer assembles credential lines then env-var lines and writes once per connect, truncating any prior copy.

### `docs/specs/06-launch-and-connect.md` → Argv assembly and `--print` (Modify)

> Under `set-env` delivery each forwarded variable becomes a `-o "SetEnv NAME=value"` argv element in the same block as the existing `SetEnv TERM=…` (after the `-o` options, before `extra_args` and the `-t … user@endpoint` tail). `set-env` **rejects** any value containing a space, tab, or newline at plan time (`ConfigInvalid`, exit 2) rather than truncating, and is **incompatible with `forward_all` and connect-resolved globs** (their names are not enumerable before connect, but `AcceptEnv` is fixed at bootstrap) — also a plan-time `ConfigInvalid`.
>
> `--print` currently shell-quotes `plan.argv` verbatim. That is safe for `env-file` (values never enter argv). It is **not** safe for `set-env`, where a verbatim dump would print values; so `--print` becomes a render that **redacts `SetEnv` values** for forwarded vars (`SetEnv NAME=<from env>`) while still printing literal `--env NAME=value` operands the operator typed and the existing `SetEnv TERM=…`. (The credential change's `env-only` delivery needs the identical redaction.)

### `docs/specs/02-cli-surface.md` → `kleya connect` / `kleya launch` (Add)

> | Flag | Effect |
> |---|---|
> | `--env <NAME[,NAME…]>` | Forward these local vars by name or glob, repeatable. Additive over `[ssh.env].forward` |
> | `--env <NAME=value>` | Set an explicit value (docker-style), repeatable; the value need not exist locally |
> | `--env-all` | Forward the operator's entire environment for this invocation (equivalent to `forward_all = true`) |
> | `--forward-secrets` | Opt out of the secret default-deny (equivalent to `deny_secrets = false`); only meaningful with `--env-all`/`forward_all`. The structural floor still applies |
> | `--no-env <NAME[,NAME…]>` | Add to the deny list for this invocation (names or globs), repeatable; wins over `--env`, `--env-all`, and config |
>
> `--env` distinguishes the two forms by the presence of `=`: `--env FOO` forwards the local value, `--env 'FOO_*'` forwards every matching local var, `--env FOO=bar` sets `FOO` to `bar` (the `=` form is an exact name, never a glob; split on the **first** `=` only). `--print` lists the forwarded **names**; values are shown only for explicit `NAME=value` flags and otherwise redacted as `NAME=<from env>` — and under `set-env` this changes `--print` from a verbatim argv dump to a `SetEnv`-redacting render (see [06](../06-launch-and-connect.md)).

### `docs/specs/05-bootstrap-rendering.md` → Template body (Modify)

> This change reuses the `~/.config/kleya/forward.env` source line in `~/.zshrc` introduced by the [credential-forwarding](2026-05-21-credential_forwarding.md) change. If the credential change has not landed, this change adds the same guarded line under a shared `enable_env_forwarding` render var (the two collapse into one "`forward.env` is sourced" toggle on merge). Under `set-env` delivery the bootstrap additionally adds `AcceptEnv` entries to `sshd_config` for the forwarded names and reloads `sshd` — the default `env-file` path needs no `sshd_config` change, which is the primary reason it is the default.

---

## Type changes

Fragment for the `[ssh.env]` config. Folds into `canonical-types.schema.json` on merge. `env` is added to `SshCfg.properties` **and** `TemplateCfg.properties` but to **neither** `required` array, so existing configs keep validating and the round-trip property test in `03` still holds.

```json
{
  "$comment": "Fragment for 2026-05-21-env_var_forwarding. Folds into canonical-types.schema.json on merge.",
  "$defs": {
    "SshEnvCfg": {
      "type": "object",
      "additionalProperties": false,
      "$comment": "Used both as the global [ssh.env] and the per-template overlay. As an overlay, scalars absent => inherit global; lists union.",
      "properties": {
        "forward":      { "type": "array", "items": { "type": "string" }, "default": [], "description": "Names or shell-glob (*) patterns." },
        "forward_all":  { "type": "boolean", "default": false },
        "deny":         { "type": "array", "items": { "type": "string" }, "default": [], "description": "Names or globs; wins over forward/forward_all." },
        "deny_secrets": { "type": "boolean", "default": true },
        "delivery":     { "type": "string", "enum": ["env-file", "set-env"], "default": "env-file" }
      }
    },
    "SshCfg": {
      "$comment": "Modified: add optional `env`. Merge into the existing SshCfg def, leaving `required` unchanged.",
      "properties": {
        "env": { "$ref": "#/$defs/SshEnvCfg" }
      }
    },
    "TemplateCfg": {
      "$comment": "Modified: add optional `env` overlay. Merge into the existing TemplateCfg def, leaving `required` unchanged.",
      "properties": {
        "env": { "$ref": "#/$defs/SshEnvCfg" }
      }
    }
  }
}
```

---

## Implementation notes

Pointers for the implementing agent. The `forward.env` writer and the `~/.zshrc` source line are shared with the credential change — build them once.

```
1. Add SshEnvCfg to crates/kleya-core/src/config.rs, with `env: Option<SshEnvCfg>` on both
   SshCfg and TemplateCfg (serde default None). Use Option scalars + empty-default Vec<String>
   so the overlay merge (lists union, scalars override) is expressible.
2. Add the built-in deny floor and secret-default-deny glob sets as named constants in kleya-core.
   Add a tiny shell-glob matcher (only `*`), case-sensitive, whole-name match.
3. Implement the resolution-precedence pipeline (steps 0–5) in ConnectService::plan
   (crates/kleya-core/src/commands/connect.rs). The template overlay lookup reads kleya:template
   from inst.tags — same tag bag resolve_key uses (connect.rs:121) — warn-and-degrade on miss.
4. Single-quote escaping fn (the '\'' rule) + name validation against ^[A-Za-z_][A-Za-z0-9_]*$.
   Reuse the shared forward.env writer; emit env-var lines after credential lines.
5. set-env path: reject whitespace values and forward_all/glob combinations at plan time
   (ConfigInvalid); place -o SetEnv in build_argv (connect.rs:140) beside SetEnv TERM=.
6. Change Cmd::Connect --print (crates/kleya-cli) to a SetEnv-redacting render.
7. Add --env/--env-all/--forward-secrets/--no-env to crates/kleya-cli/src/clap_args.rs;
   split --env on the first '=' only.
8. Reuse the enable_*_forwarding render var + ~/.zshrc source line in the setup_devbox.sh.j2
   template (crates/kleya-core/src/bootstrap/render.rs:37 BootstrapVars).
```

References: POSIX single-quote escaping (`'\''`); OpenSSH `SetEnv`/`AcceptEnv` (`sshd_config(5)`); docker `--env`/`-e` dual-form precedent.

---

## Merge plan

1. Apply each `Proposed changes` block to its canonical page; bump each touched page's `**Date:**` to the merge date. The `03` per-template block also **closes** the per-template-override open question on that page (move it out of Open questions into the body/Decisions).
2. Fold the `Type changes` `$defs` into `canonical-types.schema.json` (`SshEnvCfg`, and the `env` property on both `SshCfg` and `TemplateCfg` — leave both `required` arrays unchanged so the round-trip property test holds).
3. Reconcile with the credential change: a single shared `forward.env` writer and one collapsed `enable_*_forwarding` render var. If the credential change has not yet landed, this change adds the writer and source line under `enable_env_forwarding`.
4. Flip this file's `**Status:**` to `Merged`, add `**Merged:** YYYY-MM-DD`, and move it to `docs/specs/changes/merged/`.
5. Update `docs/README.md`: remove this file from the pending Change-specs list.

---

## Assumptions and open questions

**Assumptions**

- The box's login shell is `zsh` (and any `bash` invoked sources the same file), so a single shell-quoted `export` line reproduces every value; the `forward.env` source line is added to `~/.zshrc` ([05](../05-bootstrap-rendering.md)).
- The operator's shell has already removed one layer of quoting before kleya sees argv, so `--env NAME=value` is taken verbatim with no second unquote.
- The `kleya:template` tag stamped at launch is present and stable on managed instances; unmanaged instances (reached via `--instance-id`) legitimately lack it and degrade to global-only.

**Decisions**

- *Env-file is the default delivery.* **It is the only mechanism that carries arbitrary values intact and survives tmux reattach** ([06](../06-launch-and-connect.md) makes tmux the default session model). `set-env` is the opt-in zero-disk path with documented limits.
- *Single-quote shell escaping, not double-quote or ad-hoc backslashing.* **One rule covers every byte** — inside single quotes the shell interprets nothing, so only the single quote itself needs the `'\''` escape. Double-quoting would require per-character escaping of `$`, `` ` ``, `\`, `"` and still mishandle cases.
- *All forwarding off by default.* **Secure by default** — consistent with the credential change. The operator's environment is private until explicitly shared.
- *Unset named vars are skipped with a warning, not forwarded as empty.* **Forwarding `FOO=` when the operator never set `FOO` masks typos and can override a value the box's own `~/.zshrc` would set.**
- *`--env` is docker-style dual-form.* **`--env FOO` (from environment) and `--env FOO=bar` (explicit) match the most widely known precedent**, so muscle memory transfers and the `=`-splits-once rule is unsurprising.
- *A deny list, and it wins over everything.* **A subtractive filter is the only safe lever once `forward_all`/globs exist** — making deny the unconditional last step (over `forward`, `forward_all`, and explicit `--env NAME=value`) means a denied name can never leak through a more specific allow.
- *Glob (`*`) forwarding for both `forward` and `deny`.* **`MYAPP_*` / `*_TOKEN` is how operators think about their environment** — by application prefix or secret suffix. Limited to `*` (no full regex) to stay predictable.
- *`PATH` and host/shell-identity vars are a non-overridable floor, not a default.* **Forwarding the laptop's `PATH` would hide the box's cargo/nvm/`~/.local/bin` toolchain ([05](../05-bootstrap-rendering.md)); `LD_*`/`DYLD_*` are code-injection vectors; `HOME`/`SHELL`/`PWD`/`TERM` describe the wrong machine.** Even an explicit `--env PATH=…` cannot override the floor.
- *`forward_all` ships a secret default-deny, opt-out, that defers to explicit intent.* **The whole-environment sweep is exactly where a token leaks by accident, so the common secret-name globs are skipped by default** (`deny_secrets = true`). It only trims the implicit sweep, never a named var, and matches names not values — naming what you need remains the safe path.
- *`env` is per-template; the rest of `[ssh]` stays a singleton.* **Environment is the one connect setting that is genuinely box-shaped**, so it earns a per-template overlay resolved at connect time via the existing `kleya:template` tag (no new tag, no state file); a missing/stale tag degrades to global-only with a warning. This resolves the per-template-override open question deferred in [03](../03-configuration.md), for `env` only.

**Open questions**

- *Template-tag drift across machines.* The connect-time overlay depends on the loaded config containing the template named by `kleya:template`. Connecting from a second machine whose config lacks that template silently degrades to global-only (with a warning). Is that the right failure mode, or should the launch-time template `env` be stamped onto the instance so it travels with the box? Deferred — warning-and-degrade is the safe default until a concrete multi-machine complaint appears.
