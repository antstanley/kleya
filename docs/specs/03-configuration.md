# 03 — Configuration

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya` reads optional configuration from a single file in one of four formats and merges it with environment variables and CLI flags. The loader, validator, and merge order are deliberately tight: every option falls back to a built-in default so `kleya launch` works with no file and no flags.

---

## Responsibilities

1. Locate the config file via `--config` flag → `KLEYA_CONFIG` env → the canonical search list.
2. Parse the file by extension (`toml`, `yaml` / `yml`, `json`, `jsonc`) into the same `Config` struct.
3. Validate the parsed `Config` against the cross-field rules in `Config::validate`.
4. Surface the resolved path via `kleya config path` and the merged content via `kleya config show`.

The file lives in `kleya-cli` ([config_loader.rs](../../crates/kleya-cli/src/config_loader.rs)); the struct definitions and validator live in `kleya-core` ([config.rs](../../crates/kleya-core/src/config.rs)).

---

## Precedence

Highest wins:

1. **CLI flag** (`--region eu-west-1`, etc.)
2. **Environment variable**:
   - `KLEYA_REGION`, `KLEYA_PROFILE`, `KLEYA_CONFIG`, `KLEYA_LOG_FORMAT` (kleya's own surface).
   - Standard AWS SDK variables (`AWS_REGION`, `AWS_PROFILE`, `AWS_ACCESS_KEY_ID`, …) are honoured by `aws-config` itself; kleya never reads them directly.
3. **`--config <path>` file**, if supplied.
4. **First file matching the search order** (extensions probed in order):
   - `~/.config/kleya/config.toml`
   - `~/.config/kleya/config.yaml`
   - `~/.config/kleya/config.yml`
   - `~/.config/kleya/config.json`
   - `~/.config/kleya/config.jsonc`
5. **Built-in defaults** in `Config::default()`.

If no file matches and no `--config` is supplied, `Config::default()` is used directly and `Config::validate()` runs against the defaults (it always passes — the defaults are the canonical "valid" baseline).

`kleya config path` prints the resolved path or `<defaults; no file loaded>` when no file was used.

---

## Canonical schema

TOML is the canonical form (`kleya config show` always emits TOML). YAML / JSON / JSONC deserialise into the same `Config` struct via serde — the per-format step is purely syntactic.

```toml
default_region  = "eu-west-1"
default_profile = "default"

[defaults]
instance_type = "m8g.xlarge"
market        = "spot"           # "spot" | "on-demand"
spot_type     = "one-time"       # "one-time" | "persistent"
ami_alias     = "amazon-linux-2023-arm64"

[bootstrap]
# user_data_path = "~/.config/kleya/bootstrap.sh"   # optional override
install_ghostty_terminfo = true

[ssh]
user         = "ec2-user"
tmux         = true
tmux_session = "kleya"
extra_args   = []                # appended verbatim to the ssh argv

[keys]
dir              = "~/.config/kleya/keys"
default_key_name = "kleya-default"

# Zero or more template blocks. Templates referenced by name from
# `kleya launch --template <name>` or auto-created on first launch.
[[templates]]
name           = "gpu"
instance_type  = "g6.xlarge"
# ami_id           = "ami-..."           # optional; resolved from ami_alias when absent
# key_name         = "..."
# security_group_ids = ["sg-..."]
# subnet_id        = "subnet-..."

[[templates.tags]]
key   = "Project"
value = "gpu-experiments"
```

Every struct in `kleya_core::config` declares `#[serde(deny_unknown_fields)]` — typos or stale keys fail deserialization with a clear error rather than being silently dropped.

---

## Built-in defaults

Every field has a `serde` `default = "fn"` so partial files are merged against defaults at deserialize time. The defaults are the `Config::default()` impl in [config.rs](../../crates/kleya-core/src/config.rs):

| Path | Default | Notes |
|---|---|---|
| `default_region` | `"eu-west-1"` | Matches the legacy `launch.sh` |
| `default_profile` | `"default"` | Honoured by `aws-config` when no `AWS_PROFILE` is set |
| `defaults.instance_type` | `"m8g.xlarge"` | Graviton4 — matches the legacy script |
| `defaults.market` | `"spot"` | |
| `defaults.spot_type` | `"one-time"` | |
| `defaults.ami_alias` | `"amazon-linux-2023-arm64"` | Resolved via SSM at launch time |
| `bootstrap.user_data_path` | `None` | Embedded `setup_devbox.sh.j2` is used |
| `bootstrap.install_ghostty_terminfo` | `true` | Server-side `tic -x` install |
| `ssh.user` | `"ec2-user"` | AL2023 |
| `ssh.tmux` | `true` | `tmux new-session -A -s <session>` appended to argv |
| `ssh.tmux_session` | `"kleya"` | Validated against `^[a-z0-9_-]{1,63}$` at connect time |
| `ssh.extra_args` | `[]` | |
| `keys.dir` | `"~/.config/kleya/keys"` | Created at mode `0o700`; pem files at `0o600` |
| `keys.default_key_name` | `"kleya-default"` | Used as the `kleya:key` tag fallback on unmanaged-but-managed-tag-bearing instances |
| `templates` | `[]` | Zero templates is valid — the zero-config `default` template is created on demand |

The two zero-config-launch defaults that are *not* in the file (because they're resolved by the adapter, not the config) are documented in [06-launch-and-connect.md](06-launch-and-connect.md): `subnet_id` (lexicographically-first AZ of the default VPC) and `security_group_ids` (auto-created `kleya-default` SG).

---

## Validation

`Config::validate()` runs after deserialization, regardless of source format. It is the only place where bounds and regexes are checked.

- `Region::new(&self.default_region)` — must match `^[a-z]{2}-[a-z]+-[0-9]+$`.
- `KeyName::new(&self.keys.default_key_name)` — must match the `KeyName` regex (see [01-domain-model.md](01-domain-model.md)).
- `self.templates.len() <= TEMPLATES_COUNT_MAX (64)`.
- For each template:
  - `key_name`, if set, parses as a `KeyName`.
  - `tags.len() <= TAGS_PER_TEMPLATE_MAX (50)`.
  - Each `(key, value)` parses via `Tag::new` — non-empty key, byte caps from `TAG_KEY_BYTES_MAX` / `TAG_VALUE_BYTES_MAX`.
- `defaults.market` is `"spot" | "on-demand"`.
- `defaults.spot_type` is `"one-time" | "persistent"`.

Failures raise `Error::ConfigInvalid { reason }` (exit code 2). The reason string names the offending field and the rule it violated.

Two other limits apply at load time, before `validate()`:

- **File size**: `CONFIG_BYTES_MAX = 256 KiB`. Files above this cap are refused with `ConfigInvalid` rather than `Io`, so the exit code is the same as a malformed config.
- **UTF-8**: non-UTF-8 bytes surface as `ConfigInvalid { reason: "config not utf-8: ..." }`.

---

## Multi-format pipeline

```
explicit --config <path> ──┐
KLEYA_CONFIG               ├── resolved_path() ── shellexpand("~") ── fs::read
search ~/.config/kleya/    ┘                                              │
                                                                          ▼
                                                       parse_by_ext(path, text)
                                                                          │
                                                                          ▼
                                                                       Config
                                                                          │
                                                                          ▼
                                                            Config::validate() ── Ok / Err
```

`parse_by_ext` chooses on the lowercased extension:

| Extension | Parser |
|---|---|
| `.toml` | `toml::from_str` |
| `.yaml`, `.yml` | `serde_yaml::from_str` |
| `.json` | `serde_json::from_str` |
| `.jsonc` | `jsonc_parser::parse_to_serde_value` → `serde_json::from_value` |

Anything else returns `ConfigInvalid { reason: "unknown config extension: <ext>" }`. Serde / parser errors are wrapped in `ConfigInvalid { reason: "parse: <display>" }`.

**Round-trip property.** A `proptest` strategy generates valid `Config` values and asserts that exporting through TOML / YAML / JSON / JSONC and reloading via `parse_by_ext` yields the same `Config`. This is the only place a property test is sanctioned for configuration; see [08-testing.md](08-testing.md).

---

## Shell expansion

Paths supplied from config (`keys.dir`, `bootstrap.user_data_path`) and from `--config` are expanded for a leading `~/` against `$HOME`:

- `keys.dir` and `--config <path>` use the `shellexpand` crate via `shellexpand::tilde(...)`.
- `bootstrap.user_data_path` uses an in-tree `shellexpand_tilde` helper in [commands/launch.rs](../../crates/kleya-core/src/commands/launch.rs) (the `kleya-core` crate avoids the `shellexpand` dependency).

Neither expander handles `~user` or environment-variable substitution — `~/` against `$HOME` is the entire contract. Paths without `~/` are passed through verbatim.

---

## Assumptions and open questions

**Assumptions**

- `$HOME` is set. If `HOME` is unset *and* a path starts with `~/`, the expander falls back to the path verbatim; subsequent `fs::read` then surfaces `Io(NotFound)`.
- One config file per invocation. Layering multiple files is not supported and is documented as out of scope.
- The four format parsers (`toml`, `serde_yaml`, `serde_json`, `jsonc_parser`) agree on the same `serde` data model for the supported field set. The round-trip property test enforces this.

**Decisions**

- *`deny_unknown_fields` on every config struct.* **Typos must fail loud.** Silently accepting stale or misspelled keys is the most common configuration footgun; the cost of an error message naming the unknown field is small.
- *Single `Config` struct shared across formats.* **One validator, one set of defaults, one schema.** Per-format quirks live in `parse_by_ext` only; everything downstream of that line is identical regardless of source.
- *256 KiB file cap.* **Far more than any plausible kleya config (~2 KiB typical) and far less than DoS territory.** Refuses a misdirected gigabyte file with `ConfigInvalid` rather than letting `serde` chew through it.
- *Tilde expansion is `~/` only.* **No `~user` lookup, no `$VAR` substitution.** Either feature would need a `pwd` / shell-environment dependency; the limited form covers the user's home directory which is the only documented use case.

**Open questions**

- *Per-template `[ssh]` and `[bootstrap]` overrides.* Today `ssh` and `bootstrap` are top-level singletons. If a "low-latency dev box" template wants different `ssh.extra_args` than a "GPU build box" template, do we extend `TemplateCfg` or introduce template inheritance? Not blocking; revisit when a second concrete use case emerges.
- *Validation of `instance_type` strings.* The CLI accepts any string and forwards it to AWS, which returns an opaque "InvalidParameterValue" if it's wrong. Pre-validating against the documented EC2 instance-type list would catch typos earlier but would also pin the validator to AWS's catalogue; left unresolved.
- *Credentials and SSO.* The `Config` struct holds no credentials. Profile and region resolution is in this page; the full credentials story (which sources kleya supports, how SSO cached tokens flow through, what failure looks like, why kleya never drives login itself) is in [11-credentials-and-sso.md](11-credentials-and-sso.md). Not an open question per se — flagged here so readers looking for "how does kleya handle auth" land on the right page.
