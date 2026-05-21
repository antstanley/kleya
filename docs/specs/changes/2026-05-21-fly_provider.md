# Change: Fly.io provider (Machines API)

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley · **Target:** Repo-wide · **Depends on:** [2026-05-21-provider_neutral_port.md](2026-05-21-provider_neutral_port.md)

A new `kleya-fly` adapter crate, behind a `fly` cargo feature, will provision Fly.io Machines as dev boxes via the Machines REST API (`https://api.machines.dev/v1`, bearer-token auth) using `reqwest` + `serde` — no vendor SDK. It implements the universal `Compute` port plus `ImageResolver` (alias → OCI image), and declines `KeyRegistry`, `ServerTemplates`, and `NetworkDefaults` (`supports_spot() == false`). Because Fly runs OCI containers rather than cloud-init VMs, the bootstrap model changes for this provider: the toolchain is baked into a prebuilt OCI dev-box image, and per-launch kleya injects only the SSH public key (via the machine `files` field) rather than rendering cloud-init user-data. A `[providers.fly]` config block carries the Fly-specific settings (token source, org, region, image, guest size).

---

## Motivation

Fly.io is the first thin-REST provider the generalized port was built for: its Machines API is a small, clean, bearer-token JSON REST surface, so the adapter is `reqwest` + `serde` with no SDK weight. It exercises the capability-segmentation from the parent change — Fly genuinely lacks server-side launch templates, a cloud-side SSH-key registry, security groups, and spot pricing, so declining those capabilities (rather than stubbing them) is the whole point of the split.

It also surfaces the assumptions kleya baked in from EC2. Fly has no AMIs (it runs OCI images), no cloud-init/user-data (configuration is the image plus env vars plus written files), no public IPv4 by default (raw TCP needs a dedicated IP allocated through the Fly GraphQL API), and a different region vocabulary (three-letter codes like `lhr`, not `eu-west-1`). This change adopts a concrete primary path for each mismatch and records the alternatives it rejected — the design value is in confronting these explicitly, not in pretending Fly is EC2.

---

## Affected spec pages

| Canonical page | Nature of change |
|---|---|
| [`docs/specs/00-overview.md`](../00-overview.md) | Scope-summary "Cloud provider" row: Fly as the first non-AWS adapter |
| [`docs/specs/01-domain-model.md`](../01-domain-model.md) | Fly mapping notes: machine id, OCI `ImageRef`, `KeyRef::Inline` via authorized-keys injection |
| [`docs/specs/03-configuration.md`](../03-configuration.md) | New `[providers.fly]` block, defaults, validation; resolves the parent's per-provider-config open question for Fly |
| [`docs/specs/04-provider-port.md`](../04-provider-port.md) | New "Fly adapter — `FlyMachines`" section: capabilities implemented vs declined |
| [`docs/specs/05-bootstrap-rendering.md`](../05-bootstrap-rendering.md) | Fly bootstrap path: prebuilt OCI image + `files`-injected SSH key, no cloud-init |
| [`docs/specs/06-launch-and-connect.md`](../06-launch-and-connect.md) | Fly launch/connect: app + IPv4 allocation, port-22 service, `/wait` polling |
| [`docs/specs/07-error-model.md`](../07-error-model.md) | New `kleya_fly::FlyError`; `Adapter.provider = "fly"` |
| [`docs/specs/09-architecture-principles.md`](../09-architecture-principles.md) | Add `kleya-fly` crate (feature `fly`) to the graph + module table |
| [`canonical-types.schema.json`](../canonical-types.schema.json) | Add `FlyProviderCfg` and an optional `providers` block on `Config` |

Adds the new crate `crates/kleya-fly/`. Adds `Provider::Fly` to the `#[non_exhaustive]` enum.

---

## Proposed changes

### `docs/specs/04-provider-port.md` → Fly adapter — `FlyMachines` (Add)

> ## Fly adapter — `FlyMachines`
>
> [crates/kleya-fly/src/machines.rs](../../crates/kleya-fly/src/machines.rs). `FlyMachines` implements `Compute` against the Fly Machines REST API (`https://api.machines.dev/v1`) with a `reqwest::Client` carrying `Authorization: Bearer <token>`; `provider()` returns `"fly"` and `supports_spot()` returns `false`.
>
> | Port method | Fly call |
> |---|---|
> | `instance_launch` | ensure app exists (`POST /apps`), ensure a dedicated IPv4 (Fly GraphQL `allocateIpAddress`), then `POST /apps/{app}/machines` with the resolved config |
> | `instance_list` | `GET /apps/{app}/machines`, filtered by the `kleya:*` metadata |
> | `instance_describe` | `GET /apps/{app}/machines/{id}` |
> | `instance_terminate` | `POST …/machines/{id}/stop` then `DELETE …/machines/{id}` (or `DELETE?force=true`) |
> | `instance_wait_running` | `GET …/machines/{id}/wait?state=started&timeout=…`, retried under the cancellable `Deadline` via `wait_or_cancel` |
>
> Capability accessors:
> - `image_resolver() → Some` — `resolve_image(alias)` maps a kleya alias to a full OCI image ref (e.g. `kleya-devbox` → `registry.fly.io/kleya-devbox:<tag>`), via the `[providers.fly].images` map; an alias that is already a registry path passes through.
> - `key_registry() → None` — Fly has no cloud-side SSH-key store. `instance_launch` requires `LaunchSpec.key == KeyRef::Inline(public_key)` and writes it into the machine (see [05-bootstrap-rendering.md](../05-bootstrap-rendering.md)); a `KeyRef::Registered` reaching this adapter is a `kleya-core` orchestration bug, not a runtime input.
> - `server_templates() → None` — no server-side templates; `kleya-core` expands the spec inline (parent change).
> - `network_defaults() → None` — no security groups; ingress is governed by the app's `services` mapping, set per launch (see [06-launch-and-connect.md](../06-launch-and-connect.md)), not by a default firewall.
>
> The four `kleya:*` management values are written to the machine's `config.metadata` (Fly metadata is the tag analogue); `instance_list` filtering matches on `metadata["kleya:managed"] == "true"`, mirroring the EC2 tag-match semantics. `kleya:key` and `kleya:template` ride along in metadata so `connect` resolves the same way across providers.

### `docs/specs/03-configuration.md` → `[providers.fly]` block (Add)

> ## Provider-specific configuration
>
> Providers beyond AWS read settings from an optional `[providers.<name>]` block. The block is required only when `provider` selects that provider; an absent block while `provider = "fly"` is `ConfigInvalid`.
>
> ```toml
> provider = "fly"
>
> [providers.fly]
> token_env = "FLY_API_TOKEN"          # env var holding the Fly API token; never the token itself
> org       = "personal"               # Fly organisation slug
> region    = "lhr"                    # Fly region code (3 letters)
> image     = "kleya-devbox"           # OCI image alias or full registry path
> size      = "shared-cpu-2x"          # Fly guest preset; or [providers.fly.guest] for a custom guest
> ```
>
> Validation (`Config::validate`, AWS-shaped fields unchanged):
> - `token_env` is a non-empty env-var name; the token is read from that env var at dispatch time and is never stored in config or logs.
> - `org` is a non-empty Fly org slug.
> - `region` matches `^[a-z]{3}$` (Fly region codes). The AWS `default_region` newtype is not reused — Fly regions are a distinct vocabulary; `default_region` is ignored when `provider = "fly"`.
> - `size` is one of Fly's guest presets, or a `[providers.fly.guest]` table (`cpu_kind`, `cpus`, `memory_mb`) is given; the two are mutually exclusive.
> - `ssh.user` defaults to `root` for the Fly dev-box image (operators override per [ssh]).
>
> The `[defaults]` AMI/market/spot fields and `[[templates]]` security-group/subnet fields have no effect under `provider = "fly"`; requesting spot via `--market spot` against Fly raises `Error::CapabilityUnsupported { provider: "fly", capability: "spot" }` (parent change).

### `docs/specs/05-bootstrap-rendering.md` → Fly bootstrap path (Add)

> ## Bootstrap on providers without cloud-init
>
> The cloud-init user-data pipeline above is AWS-specific: Fly runs OCI containers and has no cloud-init, no user-data field, and no AMIs. For `provider = "fly"`, kleya does not render or encode `setup_devbox.sh.j2` per launch. Instead:
>
> 1. **Toolchain comes from the image.** The `[providers.fly].image` is a prebuilt OCI dev-box image that already contains the agent toolchain and an `sshd`. The contents that `setup_devbox.sh.j2` installs on AL2023 are baked in at image-build time rather than executed on first boot. `bootstrap.install_ghostty_terminfo` and `bootstrap.user_data_path` have no effect on Fly.
> 2. **SSH key comes from `files`.** `instance_launch` injects `LaunchSpec.key`'s public half by adding a `config.files` entry that writes `/root/.ssh/authorized_keys` (`raw_value` = base64 of the public key) with the correct mode, and passes any operator config via `config.env`. This replaces the EC2 key-pair import; no cloud-side key registry is used.
>
> The `USER_DATA_*` size budgets do not apply to Fly (no user-data). The `files` payload is bounded by the Fly Machines API's own limits; kleya asserts the encoded public key is well under that bound before the call.

### `docs/specs/06-launch-and-connect.md` → Fly launch and connect (Add)

> ## Fly launch and connect
>
> Fly machines are closed to the public internet by default and have no public IPv4 unless one is allocated, so the EC2 "instance gets a public DNS automatically" assumption does not hold. `FlyMachines::instance_launch` therefore:
>
> 1. Ensures the app exists (`POST /apps` with `{ app_name, org_slug }`; idempotent on "already exists").
> 2. Ensures a **dedicated IPv4** is allocated to the app (Fly GraphQL `allocateIpAddress`, `type: v4`) so raw TCP to port 22 is reachable. A shared IPv4 is insufficient — it only fronts HTTP/TLS services on the Fly proxy, not raw SSH.
> 3. Creates the machine with `config.services = [{ protocol: "tcp", internal_port: 22, ports: [{ port: 22 }] }]` (empty `handlers` = raw TCP passthrough), the resolved image, guest size, the `files`-injected SSH key, and the `kleya:*` metadata.
> 4. Sets `Instance.public_ip` to the allocated IPv4 and `public_dns` to the app's `<app>.fly.dev` name.
>
> `instance_wait_running` uses the Fly `GET …/machines/{id}/wait?state=started` endpoint under the cancellable `Deadline` rather than polling a describe call. `connect` then proceeds through the unchanged path — `probe_ssh_ready` against the IPv4 on port 22, then `execvp ssh root@<ip>` — because the dedicated IPv4 + port-22 service makes the machine a normal SSH target. There is no cloud-init wait on Fly; `--wait-bootstrap` is a no-op (the image is ready when the machine reaches `started`), logged once.

### `docs/specs/07-error-model.md` → `kleya_fly::FlyError` (Add)

> ## `kleya_fly::FlyError`
>
> [crates/kleya-fly/src/error.rs](../../crates/kleya-fly/src/error.rs).
>
> ```rust
> #[derive(Debug, thiserror::Error)]
> pub enum FlyError {
>     #[error("http transport: {0}")]
>     Http(#[from] reqwest::Error),
>     #[error("fly api error {status}: {message}")]
>     Api { status: u16, message: String },
>     #[error("decode response: {0}")]
>     Decode(String),
>     #[error("missing fly token: env var {0} is unset")]
>     MissingToken(String),
> }
>
> impl From<FlyError> for kleya_core::Error {
>     fn from(e: FlyError) -> Self {
>         kleya_core::Error::adapter("fly", e)
>     }
> }
> ```
>
> Public errors surface as `Error::Adapter { provider: "fly", source }` (the per-provider tag enabled by the parent change). `MissingToken` is raised when `[providers.fly].token_env` names an unset variable; `Api { status, message }` carries the Fly JSON error body. The app/IPv4 "already exists / already allocated" responses are treated as idempotent success inside the adapter and never surface as `Adapter`.

### `docs/specs/09-architecture-principles.md` → Add `kleya-fly` (Modify)

> A second adapter crate, `kleya-fly`, sits beside `kleya-aws` under the `fly` feature. It is a thin REST adapter — `kleya-core` + `reqwest` (rustls) + `serde`/`serde_json`, bearer-token auth, no vendor SDK — and depends on `kleya-core` only.
>
> ```
>                  ┌────────────────────────────────────────┐
>                  │             kleya-cli (bin)            │
>                  │  → kleya-aws     (feature "aws")       │  heavy: aws-config + sigv4
>                  │  → kleya-fly     (feature "fly")       │  thin: reqwest + serde
>                  └─────────────┬──────────────────────────┘
>                                ▼  provider_registry(): Provider::Fly → Arc<dyn Compute>
> ```
>
> `kleya-fly` module table:
>
> | Module | Role |
> |---|---|
> | `client` | `build_fly_client(token, endpoint_url)` — bearer-auth `reqwest::Client`; base-URL override for tests |
> | `machines` | `FlyMachines` struct + `Compute` + `ImageResolver` impls over the REST calls |
> | `api` | request/response `serde` structs for apps, machines, IP allocation |
> | `error` | `FlyError` + `From<FlyError> for kleya_core::Error` |
> | `tests/` | mock-server integration tests (e.g. `wiremock`) against recorded Fly responses |

### `docs/specs/00-overview.md` → Scope summary (Modify)

> The "Cloud provider" scope-summary row becomes: *AWS EC2 (`aws` feature) and Fly.io Machines (`fly` feature). Adapters are optional and feature-gated; the `Compute` port plus capability traits are provider-neutral. Fly is a thin-REST adapter (no vendor SDK) implementing `Compute` + `ImageResolver` only.*

### `docs/specs/01-domain-model.md` → Fly mapping (Add, under `Instance` / key types)

> On Fly, `InstanceId` holds the Fly machine id (the provider-agnostic `InstanceId` bound from the parent change accepts it; the AWS `i-…` regex is AWS-adapter-only). `ImageRef` holds an OCI image reference. There is no cloud-side key registry: `LaunchSpec.key` is always `KeyRef::Inline(PublicKey)`, written into the machine's `authorized_keys` at launch. The four `kleya:*` management values live in the machine's Fly `metadata` rather than EC2 tags, with identical key/value match semantics.

---

## Type changes

Fragment for the `[providers.fly]` config. Folds into `canonical-types.schema.json` on merge. `providers` is added to `Config.properties` but **not** to its `required` array, so existing AWS configs keep validating and the round-trip property test in `03` still holds.

```json
{
  "$comment": "Fragment for 2026-05-21-fly_provider. Folds into canonical-types.schema.json on merge. Adds FlyProviderCfg, ProvidersCfg, FlyGuest; adds optional `providers` to Config.",
  "$defs": {
    "FlyGuest": {
      "type": "object",
      "additionalProperties": false,
      "required": ["cpu_kind", "cpus", "memory_mb"],
      "properties": {
        "cpu_kind":  { "type": "string", "enum": ["shared", "performance"] },
        "cpus":      { "type": "integer", "minimum": 1 },
        "memory_mb": { "type": "integer", "minimum": 256, "multipleOf": 256 }
      }
    },
    "FlyProviderCfg": {
      "type": "object",
      "additionalProperties": false,
      "description": "Settings for provider = \"fly\". Required when the Fly provider is selected.",
      "required": ["token_env", "org", "region", "image"],
      "properties": {
        "token_env": { "type": "string", "minLength": 1, "description": "Env var name holding the Fly API token; never the token value." },
        "org":       { "type": "string", "minLength": 1, "description": "Fly organisation slug." },
        "region":    { "type": "string", "pattern": "^[a-z]{3}$", "description": "Fly region code (3 letters), e.g. lhr, iad, ord." },
        "image":     { "type": "string", "minLength": 1, "description": "OCI image alias or full registry path for the dev-box image." },
        "size":      { "type": ["string", "null"], "description": "Fly guest preset, e.g. shared-cpu-2x. Mutually exclusive with `guest`." },
        "guest":     { "anyOf": [ { "type": "null" }, { "$ref": "#/$defs/FlyGuest" } ] },
        "images":    {
          "type": "object",
          "description": "Optional alias → OCI image-ref map for ImageResolver::resolve_image.",
          "additionalProperties": { "type": "string" }
        }
      }
    },
    "ProvidersCfg": {
      "type": "object",
      "additionalProperties": false,
      "description": "Provider-specific config blocks. Each is required only when its provider is selected.",
      "properties": {
        "fly": { "$ref": "#/$defs/FlyProviderCfg" }
      }
    },
    "Config": {
      "$comment": "Modified: add optional `providers`. Shown with the new property only; merge into the existing Config def, leaving `required` unchanged.",
      "properties": {
        "providers": { "$ref": "#/$defs/ProvidersCfg" }
      }
    }
  }
}
```

---

## Implementation notes

Order: scaffold the crate and the read-only calls first (auth, list, describe), then launch/terminate, then the IPv4/bootstrap specifics.

```
1. Add crates/kleya-fly to the workspace; Cargo.toml depends on kleya-core, reqwest
   (rustls-tls, json), serde, serde_json, async-trait, thiserror, tokio. Make it an
   optional dep of kleya-cli behind feature "fly".
2. parsed_config: add Provider::Fly + parse("fly"); add FlyProviderCfg/ProvidersCfg to
   config.rs (serde default None) and the validation rules in Config::validate.
3. crates/kleya-fly/src/client.rs: build_fly_client(token, base_url) → bearer reqwest
   client; read the token from [providers.fly].token_env at dispatch time (MissingToken
   if unset). base_url override for tests.
4. api.rs: serde structs for App, Machine, MachineConfig (image, guest/size, env,
   services, files, metadata), the /wait response, and the IP-allocation GraphQL call.
5. machines.rs: impl Compute (lifecycle via the table above) + ImageResolver (alias map);
   leave key_registry/server_templates/network_defaults at the default None.
   instance_launch: ensure app → ensure dedicated IPv4 (GraphQL) → POST machine with the
   services/files/metadata; set Instance.public_ip/public_dns.
6. kleya-core launch orchestration already (parent change) passes KeyRef::Inline when
   key_registry() is None, expands the spec inline when server_templates() is None, and
   skips firewall when network_defaults() is None — verify those branches drive Fly
   correctly; no Fly-specific code in kleya-core.
7. dispatch.rs registry: add the #[cfg(feature="fly")] arm constructing FlyMachines from
   [providers.fly] + the resolved token.
8. Bootstrap: confirm provider = "fly" skips render_user_data and instead supplies the
   public key via the files mechanism in the adapter; log the install_ghostty_terminfo /
   user_data_path no-op once.
9. tests/: wiremock-backed tests for app-create idempotency, machine create/list/wait/
   destroy, and the metadata match filter. Document how the dev-box image is built/published
   (separate from the Rust build) in CONTRIBUTING.
```

References: [Fly Machines API — Working with the Machines API](https://fly.io/docs/machines/api/working-with-machines-api/), [Machines resource](https://fly.io/docs/machines/api/machines-resource/), [Apps resource](https://fly.io/docs/machines/api/apps-resource/), [Tokens](https://fly.io/docs/machines/api/tokens-resource/); Fly GraphQL `allocateIpAddress` for dedicated IPv4.

---

## Merge plan

1. Apply each `Proposed changes` block to its canonical page; bump each touched page's `**Date:**` to the merge date.
2. Fold the `Type changes` `$defs` into `canonical-types.schema.json` (`FlyProviderCfg`, `ProvidersCfg`, `FlyGuest`, and the `providers` property on `Config` — leave `Config.required` unchanged).
3. No new canonical page (the Fly adapter is described inside `04`); the new crate `crates/kleya-fly` is indexed in `09`'s workspace layout at merge.
4. Flip this file's `**Status:**` to `Merged`, add `**Merged:** YYYY-MM-DD`, and move it to `docs/specs/changes/merged/`.
5. Update `docs/README.md`: remove this file from the pending Change-specs list.

---

## Assumptions and open questions

**Assumptions**

- The parent change has shipped, so `kleya-core` launch orchestration already branches on `key_registry()` / `server_templates()` / `network_defaults()` being `None` and on `supports_spot()`. The Fly adapter adds no orchestration logic to `kleya-core`.
- A prebuilt Fly dev-box OCI image (toolchain + `sshd`) exists and is referenced by `[providers.fly].image`. Building and publishing that image is a separate pipeline from the Rust workspace build.
- The operator has a Fly account, an org slug, and an API token reachable through `[providers.fly].token_env`; kleya never drives `fly auth login`, mirroring its hands-off stance on AWS auth.
- A dedicated IPv4 (allocated via Fly GraphQL) makes raw TCP port 22 reachable, so kleya's existing `probe_ssh_ready` + `execvp ssh` path works unchanged.

**Decisions**

- *Implement only `Compute` + `ImageResolver`; decline the rest.* **Fly has no launch templates, key registry, security groups, or spot.** The parent change's capability accessors let the adapter return `None` for the absent ones instead of stubbing meaningless methods.
- *OCI dev-box image instead of cloud-init user-data.* **Fly has no cloud-init; configuration is the image plus env plus files.** Baking the toolchain into the image and injecting only the SSH key per launch is the faithful Fly equivalent of kleya's AL2023-plus-user-data model.
- *Dedicated IPv4 + raw-TCP port-22 service for SSH.* **A shared IP only fronts HTTP/TLS on the Fly proxy; raw SSH needs a dedicated IPv4.** This keeps `connect` provider-neutral (plain `ssh user@ip`) rather than shelling out to `fly ssh` / WireGuard.
- *Fly settings in `[providers.fly]`, not reusing AWS fields.* **Fly's region vocabulary, image model, and guest sizing don't fit the AWS-shaped top-level fields.** A per-provider block is the mechanism the parent change anticipated; this change defines it for Fly.

**Open questions**

- *IPv4 allocation lives in the Fly GraphQL API, not Machines REST.* The adapter needs one GraphQL call (`allocateIpAddress`) alongside the REST surface. Is a single hand-written GraphQL POST acceptable, or does it warrant a second small client module? Leaning toward one inline POST in `api.rs`; revisit if more GraphQL-only operations appear. Dedicated IPv4s may also incur cost — surface that to the operator.
- *Dev-box image provenance and `xterm-ghostty` terminfo.* Who builds, signs, and publishes the `kleya-devbox` OCI image, and at what registry/tag cadence? The ghostty-terminfo install that AL2023 does via `tic` must happen at image-build time instead. This is a pipeline question outside the Rust build; it blocks a usable Fly launch even though the adapter compiles.
- *SSH key model: `authorized_keys` vs Fly SSH certificates.* This change injects the public key into `authorized_keys` via `files`. Fly also has its own SSH CA (`fly ssh issue`). Sticking with `authorized_keys` keeps `connect` provider-neutral; confirm that does not conflict with an image whose `sshd` is configured for Fly's CA.
- *`InstanceId` regex relaxation.* Settled by the parent change: `InstanceId` relaxes to a provider-agnostic non-empty bound and each adapter re-asserts its own format (AWS keeps `^i-…`). Fly stores raw machine ids in `InstanceId` accordingly; no open question remains here beyond confirming the parent shipped that bound before this adapter lands.
- *Volumes and persistence.* Fly machines are ephemeral without a volume. A dev box that survives a stop/start may want a Fly volume mounted at the home dir. Out of scope here; flagged for a follow-up if operators want persistent Fly boxes.
