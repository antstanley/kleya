# 05 — Bootstrap Rendering

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya` bootstraps each instance with a single user-data shell script that the EC2 instance executes via cloud-init on first boot. The script and the `ghostty.terminfo` source it installs are embedded in the binary via `include_str!` at compile time — no network fetch at runtime. Rendering and encoding are pure functions in `kleya-core::bootstrap`; they have no I/O dependencies.

---

## Responsibilities

1. Embed the `setup_devbox.sh.j2` minijinja template and the `ghostty.terminfo` source as compile-time constants.
2. Render the template into a complete shell script via `bootstrap::render::render(vars)`.
3. Encode the script for the EC2 `user-data` wire format (gzip + base64 by default; base64-only when the operator supplies an override script).
4. Enforce the EC2 16 KiB user-data limit at the right boundary so an oversized script returns `Error::UserDataTooLarge` before any AWS call.

The assets live in [crates/kleya-bootstrap-assets/](../../crates/kleya-bootstrap-assets/); the rendering and encoding functions live in [crates/kleya-core/src/bootstrap/](../../crates/kleya-core/src/bootstrap/).

---

## Embedded assets

```rust
// crates/kleya-bootstrap-assets/src/lib.rs
pub const SETUP_TEMPLATE: &str   = include_str!("../assets/setup_devbox.sh.j2");
pub const GHOSTTY_TERMINFO: &str = include_str!("../assets/ghostty.terminfo");
```

A non-empty assertion test guards the two constants. The `ghostty.terminfo` file carries a header comment with the source commit SHA from `ghostty-org/ghostty`; bumping it is an ordinary commit.

---

## Template variables

[crates/kleya-core/src/bootstrap/render.rs](../../crates/kleya-core/src/bootstrap/render.rs).

```rust
pub struct BootstrapVars<'a> {
    pub install_ghostty_terminfo: bool,
    pub ghostty_terminfo_source: &'a str,
    pub install_dev_tools: bool,
    pub node_major: u8,
    pub python_version: &'a str,
    pub extra_pre_lines: &'a [String],
    pub extra_post_lines: &'a [String],
}
```

The default constructor `BootstrapVars::default_with(GHOSTTY_TERMINFO)` sets:

| Field | Default |
|---|---|
| `install_ghostty_terminfo` | `true` |
| `ghostty_terminfo_source` | `kleya_bootstrap_assets::GHOSTTY_TERMINFO` |
| `install_dev_tools` | `false` |
| `node_major` | `24` |
| `python_version` | `"3.12"` |
| `extra_pre_lines` / `extra_post_lines` | empty slices |

`render(vars)` calls `render_with(SETUP_TEMPLATE, vars)`; tests use the same `render_with` against inline test templates for fine-grained assertions. Two preconditions are asserted:

```rust
assert!(!template.is_empty(), "bootstrap template empty");
assert!(vars.node_major >= 18, "node_major too low");
```

The post-condition is `assert!(!out.is_empty(), "rendered output empty")`. minijinja errors are wrapped in `Error::ConfigInvalid { reason: "template render: <e>" }`.

---

## Template body

The committed `setup_devbox.sh.j2` is the gist's `setup_devbox.sh` with two structural changes:

1. `{% if install_ghostty_terminfo %} … {% endif %}` and `{% if install_dev_tools %} … {% endif %}` blocks wrap the terminfo install and the dev-tool installs respectively, so either block can be disabled without recompiling.
2. A heredoc writes the embedded `ghostty.terminfo` to `/tmp/ghostty.terminfo` and runs `sudo tic -x /tmp/ghostty.terminfo`, so `xterm-ghostty` lands in the system terminfo database. Sketch:

```bash
{% if install_ghostty_terminfo %}
sudo dnf install -y ncurses
cat > /tmp/ghostty.terminfo <<'GHOSTTY_TERMINFO_EOF'
{{ ghostty_terminfo_source }}
GHOSTTY_TERMINFO_EOF
sudo tic -x /tmp/ghostty.terminfo
rm /tmp/ghostty.terminfo
{% endif %}
```

The full template body is snapshot-tested via `insta` against the two rendered outputs `setup_devbox_default` (everything on) and `setup_devbox_no_ghostty` (ghostty block disabled) in [crates/kleya-core/tests/render_snapshot.rs](../../crates/kleya-core/tests/render_snapshot.rs). Snapshots are updated with `cargo insta review` only after a deliberate change to the template or its variables.

---

## Encoding pipeline

[crates/kleya-core/src/bootstrap/encode.rs](../../crates/kleya-core/src/bootstrap/encode.rs) exposes two functions:

```rust
pub fn encode_user_data(raw: &str) -> Result<String>;             // default: gzip + base64
pub fn encode_user_data_passthrough(raw: &str) -> Result<String>; // override: base64 only
```

### Default (gzip + base64) path

Used when no `bootstrap.user_data_path` is set. EC2 detects gzip-compressed user-data via the gzip magic bytes; cloud-init decompresses before executing. The operative ceiling is the **gzipped** size, since that is what EC2 stores.

```
preflight: raw.len() <= USER_DATA_RAW_BYTES_MAX * 4   (64 KiB)
         → else Error::UserDataTooLarge { bytes: raw.len(), max: 64 KiB }
gzip:    Compression::best()
postflight: gz.len() <= USER_DATA_GZIP_BYTES_MAX      (16 KiB)
         → else Error::UserDataTooLarge { bytes: gz.len(), max: 16 KiB }
base64:  STANDARD engine
debug_assert!(b64.len() <= USER_DATA_BASE64_BYTES_MAX)
```

The 64 KiB raw preflight is a cheap short-circuit that refuses pathological inputs before allocating a gzip buffer. The 16 KiB gzip cap is the EC2-enforced limit on the compressed wire bytes.

### Passthrough (base64 only) path

Used when the operator supplies an override script via `bootstrap.user_data_path` (or `kleya template create --user-data <path>`). The script is read as UTF-8, base64-encoded, and sent verbatim — no templating, no gzip:

```
size:    raw.len() <= USER_DATA_RAW_BYTES_MAX        (16 KiB)
         → else Error::UserDataTooLarge { bytes: raw.len(), max: 16 KiB }
base64:  STANDARD engine
debug_assert!(b64.len() <= USER_DATA_BASE64_BYTES_MAX)
```

The 16 KiB cap is the operative EC2 limit when gzip is not in use. Non-UTF-8 bytes surface as `Error::ConfigInvalid { reason: "user-data not utf-8: ..." }` at the file-read site, not in `encode_user_data_passthrough`.

### Size budget table

| Limit | Value | Where checked | Why |
|---|---|---|---|
| `USER_DATA_RAW_BYTES_MAX` | 16 KiB | passthrough path; preflight in gzip path (× 4) | Operative EC2 cap when the script is not gzipped |
| `USER_DATA_GZIP_BYTES_MAX` | 16 KiB | post-gzip in default path | EC2 enforced on the compressed bytes |
| `USER_DATA_BASE64_BYTES_MAX` | 21 848 | `debug_assert!` only | 4/3 × 16 KiB rounded up — sanity bound on the base64 form; not EC2-enforced (base64 is transport encoding) |

Negative-space tests: padding `extra_post_lines` past the operative limit returns `Error::UserDataTooLarge { bytes, max }`; the test names are `rejects_when_gzip_exceeds_cap`, `passthrough_rejects_oversize_raw_input`, and `rejects_oversize_raw_input`.

---

## Override semantics

`bootstrap.user_data_path` in config (or `--user-data <path>` on `kleya template create`) supplies an opaque shell script that replaces the embedded template entirely.

- The file is read as UTF-8; non-UTF-8 returns `Error::ConfigInvalid`.
- Size + encoding checks still apply (passthrough path).
- Templating is skipped — the script is operator-supplied, byte-exact.
- `install_ghostty_terminfo` has no effect when an override is in use. If both are set, `LaunchService::render_user_data` logs `tracing::warn!("bootstrap.user_data_path is set; install_ghostty_terminfo has no effect")` once per launch.

---

## End-to-end flow

```
config + flags ─▶ build BootstrapVars (or read override file)
                             │
                             ├──override set?──┐
                             │                  ▼
                             │       encode_user_data_passthrough(raw)
                             │                  │
                             ▼                  ▼
              render(SETUP_TEMPLATE, vars)      base64
                             │                  │
                             ▼                  │
              encode_user_data(rendered)        │
                             │                  │
                       gzip → base64            │
                             │                  │
                             ▼                  ▼
                            TemplateSpec.user_data_base64
                                          │
                                          ▼
                            CloudCompute::ensure_default_template / template_create
                                          │
                                          ▼
                                       EC2 RunInstances
                                          │
                                          ▼
                                 cloud-init on the instance
```

The pure functions `render` and `encode_user_data*` are called from both [commands/launch.rs](../../crates/kleya-core/src/commands/launch.rs)'s `LaunchService::render_user_data` (for `ensure_default_template`) and the CLI's [dispatch.rs](../../crates/kleya-cli/src/dispatch.rs)'s `build_template_spec` (for explicit `template create / update`). The two call sites use the same encoder so behaviour matches across the surface.

---

## Assumptions and open questions

**Assumptions**

- The target OS is Amazon Linux 2023. The script uses `dnf`, `sudo`, and `tic` from `ncurses`. Other distributions would need a different script — operator's responsibility via override.
- cloud-init runs the user-data on first boot. AL2023 ships cloud-init by default; the script is idempotent on re-run but the only invocation path that matters is the first one.
- The Ghostty terminfo file at `assets/ghostty.terminfo` is the upstream `ghostty-org/ghostty` source verbatim. `tic -x` accepts it on AL2023.
- Operators using `--user-data <path>` know that their script will be executed verbatim by cloud-init; kleya does not lint shebangs or shell syntax.

**Decisions**

- *Embed the user-data and terminfo in the binary.* **No network fetch at launch.** A fresh instance is one base64-decoded blob away from a working environment; nothing to lose if `raw.githubusercontent.com` is having a bad day.
- *gzip by default, base64-passthrough for overrides.* **The 16 KiB EC2 cap is the binding constraint; gzip buys ~3-4× headroom on the canonical script.** Operator-supplied scripts are opaque, so we don't add gzip headers they may not expect — pass them through.
- *minijinja for templating.* **Lighter than full Jinja2 (no Python dep), supports the `{% if %}` blocks we need.** Errors fold into `Error::ConfigInvalid` so the template render path doesn't surface a new error variant.
- *Snapshot the rendered script with `insta`.* **A template change without a corresponding snapshot review is a CI failure.** The rendered shell is what runs on production boxes; any silent change to it is a regression risk.
- *`Compression::best()`, not `Compression::default()`.* **Build-time cost on a 16 KiB script is negligible (~milliseconds), and the headroom matters.** A future template addition that pushes us past 16 KiB raw is easier to absorb at best compression than at default.

**Open questions**

- *Bootstrap script vendoring.* Resolved (action): the current forked-gist `setup_devbox.sh.j2` is not maintainable as-is and needs to be revisited. A follow-up task will choose between consuming the upstream as a git submodule or as a versioned tarball.
- *Per-template `install_dev_tools`.* Deferred: the flag stays internal (`false` by default, not exposed via config) until a second template type emerges (e.g., a minimal "ssh-only" box).
- *`tic -x` exit code handling.* Resolved: make it strict. `tic` failure aborts bootstrap rather than being treated as best-effort, so missing `xterm-ghostty` terminfo never reaches a "successful" instance silently.
