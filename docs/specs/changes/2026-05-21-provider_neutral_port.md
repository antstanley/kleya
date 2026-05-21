# Change: Provider-neutral compute port

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley · **Target:** Repo-wide

The single AWS-shaped `CloudCompute` trait will be split into a small **universal `Compute`** port that every provider implements (VM lifecycle only) plus a set of **optional capability traits** (`KeyRegistry`, `ImageResolver`, `ServerTemplates`, `NetworkDefaults`) that the AWS-specific concepts — launch templates, security groups, subnets, AMI/SSM resolution, cloud-side key import — retreat into. Launch input becomes a fully-resolved, provider-neutral `LaunchSpec` rather than a launch-template name plus `RunInstances` overrides. The AWS adapter implements the core port and all four capabilities; a thin-REST adapter (Hetzner, Fly, DigitalOcean, …) implements only `Compute` and whichever capabilities its API actually has, declining the rest. Provider adapter crates become optional, feature-gated dependencies of `kleya-cli`, so a build links only the SDKs for the providers it ships.

---

## Motivation

The current `CloudCompute` ([04-provider-port.md](../04-provider-port.md)) is EC2-shaped: `template_create`/`ensure_default_template` assume server-side launch templates, `ensure_default_security_group` assumes security groups, `resolve_default_subnet` assumes VPC subnets, `resolve_ami_alias` assumes AMIs resolved through SSM, and `ensure_default_keypair`/`keypair_fingerprint` assume a cloud-side key registry. None of these concepts is universal. A Hetzner, Fly, DigitalOcean, Vultr, or Linode adapter would have to implement a dozen methods that have no meaning on its API, returning errors or no-ops for most of them — the trait would lie about what each provider can do.

The goal is 10+ providers without a bloated binary. Two things block that today: the port forces every provider to pretend to be EC2, and `kleya-cli` depends on `kleya-aws` (and therefore the multi-megabyte `aws-sdk-*` stack) unconditionally. This change fixes the first by segmenting the port along capability lines, and the second by making each provider crate an optional cargo feature so a build pulls in only the adapters — and SDKs — it enables. Most providers can then ship as thin `reqwest`-based adapters that implement `Compute` plus one or two capabilities; AWS keeps its SDK behind the default-on `aws` feature.

---

## Affected spec pages

| Canonical page | Nature of change |
|---|---|
| [`docs/specs/00-overview.md`](../00-overview.md) | Goal/Non-goal wording, system-shape diagram, scope-summary "Cloud provider" row |
| [`docs/specs/01-domain-model.md`](../01-domain-model.md) | Generalise `Instance`; add `ImageRef`, `Placement`, `FirewallRef`, `KeyRef`, `Market`; replace `LaunchRequest` with `LaunchSpec`; demote `AmiId`/`SubnetId`/`SecurityGroupId` to adapter-internal |
| [`docs/specs/04-provider-port.md`](../04-provider-port.md) | Split `CloudCompute` into `Compute` + four capability traits; capability accessors; per-capability idempotency contract; AWS adapter implements all |
| [`docs/specs/06-launch-and-connect.md`](../06-launch-and-connect.md) | Rewrite `LaunchService::run` as capability-aware orchestration (resolve image, ensure key, ensure firewall/template only when supported) |
| [`docs/specs/07-error-model.md`](../07-error-model.md) | Add `Error::CapabilityUnsupported`; generalise `Adapter.provider` from always-`"aws-ec2"` to a per-provider tag |
| [`docs/specs/09-architecture-principles.md`](../09-architecture-principles.md) | Feature-gated optional provider crates; thin-REST adapters; registry-based dispatch; AWS SDK behind the `aws` feature |
| [`canonical-types.schema.json`](../canonical-types.schema.json) | Add `LaunchSpec`, `ImageRef`, `Placement`, `FirewallRef`, `KeyRef`, `Market`, `Capabilities`; modify `Instance`, `Error`, `ExitCode`; retire `LaunchRequest` |

This change does **not** add any new provider adapter crate. It establishes the port shape and the feature-gating mechanism; each thin-REST adapter (`kleya-hetzner`, `kleya-fly`, …) is a separate follow-up change that lands a crate behind its own feature. The CLI surface ([02-cli-surface.md](../02-cli-surface.md)) and the config schema ([03-configuration.md](../03-configuration.md)) are unchanged: the `provider` config field and `--provider` flag already exist.

**Downstream changes.** Two child changes depend on this one and are its first consumers: [2026-05-21-aws_thin_rest_adapter.md](2026-05-21-aws_thin_rest_adapter.md) swaps the AWS adapter's transport for a `reqwest` + `aws-sigv4` client beneath these capability traits, and [2026-05-21-fly_provider.md](2026-05-21-fly_provider.md) adds Fly.io as the first thin-REST provider (implementing `Compute` + `ImageResolver` only). Validating this design against those two surfaced two refinements applied below — `LaunchSpec.user_data_base64` is optional (not every provider uses cloud-init) and `Placement` carries no region (region pins the adapter, not the launch) — and resolves several open questions.

---

## Proposed changes

One subsection per affected page. Each block is the prose as it should read once merged.

### `docs/specs/04-provider-port.md` → opening + Responsibilities (Modify)

> `kleya-core` describes the cloud provider as a small universal port — `Compute` — plus four optional **capability traits** that providers implement only when their API supports the concept. `KeyStore` (local key material) is unchanged. The AWS implementation in `kleya-aws` implements `Compute` and all four capabilities; a thin-REST adapter implements `Compute` and only the capabilities its provider actually has. `kleya-core` never imports a provider SDK.
>
> Responsibilities:
> 1. Define a universal compute surface — instance launch, list, describe, terminate, wait-for-running — that every provider can satisfy.
> 2. Express provider-specific concepts (server-side launch templates, firewalls, placement/subnets, image-alias resolution, cloud-side key registries) as **optional capabilities**, discoverable at runtime, rather than mandatory methods.
> 3. Make idempotency a contract on each capability that has an `ensure_*` method.
> 4. Translate provider errors into `kleya_core::Error::Adapter { provider, source }` at the boundary; surface a requested-but-absent capability as `Error::CapabilityUnsupported { provider, capability }`.

### `docs/specs/04-provider-port.md` → `CloudCompute` trait (Modify → `Compute` trait + capabilities)

> ## `Compute` trait
>
> [crates/kleya-core/src/ports/compute.rs](../../crates/kleya-core/src/ports/compute.rs).
>
> ```rust
> #[async_trait]
> pub trait Compute: Send + Sync {
>     /// Stable provider tag, e.g. "aws-ec2", "hetzner", "fly". Used as the
>     /// `provider` field on `Error::Adapter` and `Error::CapabilityUnsupported`.
>     fn provider(&self) -> &'static str;
>
>     /// Whether spot/preemptible pricing is honoured. Launch ignores
>     /// `LaunchSpec.market` when false.
>     fn supports_spot(&self) -> bool { false }
>
>     // Universal VM lifecycle — every provider implements these.
>     async fn instance_launch(&self, spec: &LaunchSpec) -> Result<Instance>;
>     async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>>;
>     async fn instance_describe(&self, id: &InstanceId) -> Result<Instance>;
>     async fn instance_terminate(&self, id: &InstanceId) -> Result<()>;
>     async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance>;
>
>     // Optional capabilities — default None; a provider returns Some(self)
>     // (or Some(&field)) for each capability it implements.
>     fn key_registry(&self)    -> Option<&dyn KeyRegistry>    { None }
>     fn image_resolver(&self)  -> Option<&dyn ImageResolver>  { None }
>     fn server_templates(&self) -> Option<&dyn ServerTemplates> { None }
>     fn network_defaults(&self) -> Option<&dyn NetworkDefaults> { None }
> }
> ```
>
> The trait stays object-safe and is held as `Arc<dyn Compute>`. A capability is "supported" iff its accessor returns `Some`; orchestration that needs a capability calls the accessor and either uses it or, when the capability was explicitly requested by config, raises `Error::CapabilityUnsupported`.
>
> ### Capability traits
>
> [crates/kleya-core/src/ports/capabilities.rs](../../crates/kleya-core/src/ports/capabilities.rs).
>
> ```rust
> /// Cloud-side SSH key registry. Providers that inject the public key inline
> /// at launch (`LaunchSpec.key = KeyRef::Inline`) do not implement this.
> #[async_trait]
> pub trait KeyRegistry: Send + Sync {
>     async fn ensure_key(&self, name: &KeyName, public_key: &PublicKey) -> Result<()>;
>     async fn key_fingerprint(&self, name: &KeyName) -> Result<Option<Fingerprint>>;
>     async fn key_delete(&self, name: &KeyName) -> Result<()>;
>     /// Stable lowercase kebab-case label for the fingerprint format, e.g.
>     /// "md5-spki-ed25519" (AWS EC2). Lets the local `KeyStore` match formats.
>     fn fingerprint_algorithm(&self) -> &'static str;
> }
>
> /// Resolve a stable image alias to a provider-native image reference.
> #[async_trait]
> pub trait ImageResolver: Send + Sync {
>     async fn resolve_image(&self, alias: &str) -> Result<ImageRef>;
> }
>
> /// Durable server-side launch templates (AWS launch templates). Providers
> /// without server-side templates omit this; `kleya` expands the spec inline
> /// at launch instead.
> #[async_trait]
> pub trait ServerTemplates: Send + Sync {
>     async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId>;
>     async fn template_update(&self, id: &TemplateId, spec: &TemplateSpec) -> Result<TemplateVersion>;
>     async fn template_list(&self) -> Result<Vec<TemplateSummary>>;
>     async fn template_delete(&self, id: &TemplateId) -> Result<()>;
>     async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>>;
>     async fn ensure_default_template(&self, spec: &TemplateSpec) -> Result<TemplateId>;
> }
>
> /// Default network resources: a firewall/security-group and a placement
> /// (subnet / zone) pick.
> #[async_trait]
> pub trait NetworkDefaults: Send + Sync {
>     async fn ensure_default_firewall(&self, name: &str) -> Result<FirewallRef>;
>     async fn resolve_default_placement(&self) -> Result<Placement>;
> }
> ```

### `docs/specs/04-provider-port.md` → Idempotency contract (Modify)

> Idempotency is a contract on every `ensure_*` method across the capability traits. `KeyRegistry::ensure_key`, `NetworkDefaults::ensure_default_firewall`, and `ServerTemplates::ensure_default_template` must each succeed under repeated invocation with the same arguments, treating provider-side "already exists" responses as success **after a follow-up describe confirms the existing resource matches**. The adapter owns this; `kleya-core` orchestration is straight-line with no retry. The AWS adapter's specific handling (`InvalidGroup.Duplicate`, `InvalidKeyPair.Duplicate`, `InvalidPermission.Duplicate`, and the confirm-after-import `DescribeKeyPairs`) is unchanged — it now lives inside the `NetworkDefaults` and `KeyRegistry` impls rather than on the monolithic trait.

### `docs/specs/04-provider-port.md` → AWS adapter (Modify)

> ## AWS adapter — `AwsEc2`
>
> [crates/kleya-aws/src/ec2.rs](../../crates/kleya-aws/src/ec2.rs). `AwsEc2` implements `Compute` and all four capability traits, so `key_registry`, `image_resolver`, `server_templates`, and `network_defaults` each return `Some(self)`; `supports_spot` returns `true`. The instance lifecycle, launch-template builder mapping (`build_request_launch_template_data`), `RunInstances` tagging, cancellation in poll loops, and error surface are unchanged from the pre-split adapter — only their grouping into capability traits is new. The AWS-only newtypes `AmiId`, `SubnetId`, and `SecurityGroupId` are now adapter-internal mapping targets: `ImageResolver::resolve_image` returns an `ImageRef` wrapping the resolved `ami-…`; `NetworkDefaults` maps `FirewallRef`/`Placement` to/from `sg-…`/`subnet-…`.

### `docs/specs/01-domain-model.md` → `Instance` entity (Modify)

> ### `Instance` ([model/instance.rs](../../crates/kleya-core/src/model/instance.rs))
>
> Represents a managed cloud VM, mapped at the adapter boundary from the provider's native instance type. `InstanceId`'s regex stays EC2-shaped (`^i-[0-9a-f]{8,32}$`) on the AWS adapter; other adapters validate their own id format at their boundary and store it in the same `InstanceId` newtype, whose pattern relaxes to a provider-agnostic non-empty bound. `state`, `public_dns`, `public_ip`, and `tags` are unchanged.

### `docs/specs/01-domain-model.md` → New launch-input types (Add)

> ### Provider-neutral launch types ([model/launch.rs](../../crates/kleya-core/src/model/launch.rs), [model/placement.rs](../../crates/kleya-core/src/model/placement.rs))
>
> `LaunchSpec` is the fully-resolved, provider-neutral input to `Compute::instance_launch`. It replaces the former `LaunchRequest` (which named a server-side launch template and carried `RunInstances` overrides):
>
> - `name: InstanceName`
> - `image: ImageRef` — a resolved provider-native image reference (`ImageResolver::resolve_image` produces it; an operator-supplied raw id bypasses resolution)
> - `size: String` — instance type / machine type, passed verbatim to the provider
> - `key: KeyRef` — `Registered(KeyName)` for providers with a key registry, or `Inline(PublicKey)` for providers that take the public key at launch
> - `placement: Option<Placement>` — `{ zone, subnet }`, all optional; absent means provider default. Region is **not** here: it pins the adapter at construction (AWS `default_region`, Fly `[providers.fly].region`), so it is a per-provider client concern, not a per-launch field — and keeping it out avoids forcing the AWS-shaped `Region` newtype onto provider-neutral placement
> - `firewall: Option<FirewallRef>` — opaque provider firewall handle; absent means provider default
> - `market: Option<Market>` — `OnDemand` or `Spot { spot_type }`; ignored by adapters whose `supports_spot()` is false
> - `tags: Vec<Tag>` — the four `kleya:*` management tags are added by the adapter at launch, as today
> - `user_data_base64: Option<String>` — cloud-init user-data for providers that consume it (AWS); `None` for providers that bootstrap from a prebuilt image and deliver the key by other means (Fly bakes the toolchain into an OCI image and injects the public key via the machine `files` field). See [05-bootstrap-rendering.md](../05-bootstrap-rendering.md)
>
> `ImageRef(String)`, `FirewallRef(String)`, and `Placement` are provider-neutral. The AWS-specific `AmiId`, `SubnetId`, and `SecurityGroupId` no longer appear on the port; they survive only inside `kleya-aws` as mapping targets. `TemplateSpec` is now consumed by the `ServerTemplates` capability rather than the core port.

### `docs/specs/06-launch-and-connect.md` → `LaunchService::run` orchestration (Modify)

> `LaunchService::run` resolves a provider-neutral plan and branches on the adapter's capabilities rather than calling EC2-specific methods unconditionally:
>
> ```
> 1. build_plan(opts)
>    ├── InstanceName = opts.instance_name OR id_gen.name()
>    ├── KeyName      = config.keys.default_key_name (validated)
>    ├── image        = compute.image_resolver()
>    │                    .map(|r| r.resolve_image(config.defaults.ami_alias))   ← if supported
>    │                    .unwrap_or_else(|| ImageRef::from(raw_image_from_config))
>    └── market       = compute.supports_spot().then(|| plan.market)
>
> 2. if opts.dry_run: log resolved plan; return Ok(None)
>
> 3. ensure_keypair(&plan.key_name)            ← every launch (see lifecycle)
>      ├── compute.key_registry() == Some → KeyRegistry path (import + fingerprint check)
>      └── compute.key_registry() == None → KeyRef::Inline(local public key); no cloud check
>
> 4. firewall = compute.network_defaults()
>                  .map(|n| n.ensure_default_firewall("kleya-default")).transpose()?
>    placement = compute.network_defaults()
>                  .map(|n| n.resolve_default_placement()).transpose()?
>
> 5. if compute.server_templates() == Some:
>        ensure_default_template(spec) and launch references it          ← AWS path
>    else:
>        expand the spec inline into the LaunchSpec                       ← thin-provider path
>
> 6. instance_launch(LaunchSpec { name, image, size, key, placement, firewall, market, tags, user_data })
>
> 7. instance_wait_running(inst.id, Deadline { … })
> ```
>
> When config explicitly requests a capability the provider lacks — a spot market on a provider whose `supports_spot()` is false, or an explicit firewall on a provider with no `NetworkDefaults` — `build_plan` raises `Error::CapabilityUnsupported { provider, capability }` before any provisioning. Absent-but-not-requested capabilities are silently skipped (a provider with no firewall concept simply launches without one). The keypair lifecycle table in [01-domain-model.md](../01-domain-model.md#lifecycle-keypair) applies only on the `KeyRegistry` path; the inline path writes the local key and passes its public half in `LaunchSpec`.

### `docs/specs/07-error-model.md` → `Error` enum (Add + Modify)

> Add one variant for a requested-but-unsupported capability, and generalise the `Adapter` provider tag:
>
> ```rust
>     #[error("provider {provider} does not support {capability}")]
>     CapabilityUnsupported { provider: &'static str, capability: &'static str },
>
>     #[error("adapter {provider}: {source}")]
>     Adapter { provider: &'static str, #[source] source: BoxError },
> ```
>
> `CapabilityUnsupported` is raised in `LaunchService::build_plan` when config asks for a capability the selected provider's adapter does not implement (`capability` is one of `"spot"`, `"server-templates"`, `"network-defaults"`, `"key-registry"`, `"image-resolver"`). It maps to exit code **9**. The `Adapter.provider` tag is no longer always `"aws-ec2"`: each adapter passes its own `Compute::provider()` value (`"aws-ec2"`, `"hetzner"`, `"fly"`, …), so the public error names which provider failed. This resolves the deferred "Provider tag on Adapter" open question.

### `docs/specs/07-error-model.md` → Exit-code mapping (Modify)

> ```rust
>         Error::CapabilityUnsupported { .. } => 9,
> ```
>
> Code 9 follows the existing convention of one code per remediation class (after `CloudInitFailed = 8`); the operator's remediation is "select a provider that supports this, or drop the option from config".

### `docs/specs/09-architecture-principles.md` → Dependency graph + adapter conventions (Modify)

> Provider adapter crates are **optional, feature-gated dependencies of `kleya-cli`**. Each provider has a cargo feature named after it (`aws`, `hetzner`, `fly`, …); the matching crate (`kleya-aws`, `kleya-hetzner`, …) is an `optional = true` dependency enabled by that feature. `aws` is in `default`. A build links only the adapters — and their SDKs — for the features it enables, so a Hetzner-only build never pulls in `aws-sdk-*`.
>
> ```
>                  ┌────────────────────────────────────────┐
>                  │             kleya-cli (bin)            │
>                  │  → kleya-core                          │
>                  │  → kleya-aws       (feature "aws")     │
>                  │  → kleya-hetzner   (feature "hetzner") │  ← thin: reqwest + serde
>                  │  → kleya-fly       (feature "fly")     │  ← thin: reqwest + serde
>                  └─────────────┬──────────────────────────┘
>                                │ provider_registry(): ProviderId → Arc<dyn Compute>
>                                ▼
>                         (one #[cfg(feature)] arm per enabled adapter)
> ```
>
> `kleya-cli/src/dispatch.rs` replaces the hardcoded `match Provider::Aws` with a registry: a `provider_registry()` function whose arms are `#[cfg(feature = "<provider>")]`-gated, each mapping a `Provider` variant to an adapter constructor. Selecting a provider whose feature was not compiled in yields `Error::ConfigInvalid` naming the compiled-in features. Heavy-SDK adapters (`kleya-aws`) and thin-REST adapters (`reqwest` against the provider's HTTP API, no vendor SDK) sit side by side; both depend on `kleya-core` only and implement `Compute` plus whatever capabilities apply. The rule that `kleya-core` never references any provider SDK is unchanged.

### `docs/specs/00-overview.md` → Goals / Non-goals / Scope (Modify)

> Goal 4 becomes: *Provider-neutral compute port (`Compute`) in `kleya-core` with optional capability traits; the EC2 implementation lives in `kleya-aws` behind the `aws` feature. Adding a provider is a new feature-gated crate implementing `Compute` plus the capabilities its API supports.*
>
> Non-goal 4 becomes: *Thin-REST adapters for non-AWS providers (the port and feature-gating exist; each adapter crate lands in a later change).*
>
> The "Cloud provider" scope-summary row becomes: *AWS EC2 via `aws-sdk-ec2` + `aws-sdk-ssm`, behind the default-on `aws` feature. The `Compute` port plus capability traits are provider-neutral; adapter crates are optional and feature-gated.* The system-shape diagram's `kleya-aws` box is annotated as feature-gated and the port edge is labelled `Arc<dyn Compute>`.

---

## Type changes

Fragment for the provider-neutral launch and capability types. Folds into `canonical-types.schema.json` on merge. `LaunchRequest` is removed; `Instance`, `Error`, and `ExitCode` are modified.

```json
{
  "$comment": "Fragment for 2026-05-21-provider_neutral_port. Folds into canonical-types.schema.json on merge. Removes LaunchRequest; modifies Instance, Error, ExitCode.",
  "$defs": {
    "ImageRef": {
      "type": "string",
      "minLength": 1,
      "description": "Provider-native image reference (AWS: an ami-… id; thin providers: an image slug). Produced by ImageResolver or supplied raw."
    },
    "FirewallRef": {
      "type": "string",
      "minLength": 1,
      "description": "Opaque provider firewall / security-group handle. AWS maps this to an sg-… id."
    },
    "Placement": {
      "type": "object",
      "additionalProperties": false,
      "description": "Provider-neutral intra-region placement hint. All fields optional; absent means provider default. AWS maps `subnet` to a subnet-… id. Region is not here — it pins the adapter at construction, not the launch.",
      "properties": {
        "zone":   { "type": ["string", "null"] },
        "subnet": { "type": ["string", "null"] }
      }
    },
    "Market": {
      "type": "object",
      "additionalProperties": false,
      "description": "Pricing model. Ignored by adapters whose supports_spot() is false.",
      "required": ["kind"],
      "properties": {
        "kind":      { "$ref": "#/$defs/MarketKind" },
        "spot_type": { "anyOf": [ { "type": "null" }, { "$ref": "#/$defs/SpotType" } ] }
      }
    },
    "KeyRef": {
      "description": "How the launch references its SSH key: a name registered with the provider's KeyRegistry, or an inline public key for providers without a registry.",
      "oneOf": [
        { "type": "object", "additionalProperties": false, "required": ["Registered"],
          "properties": { "Registered": { "$ref": "#/$defs/KeyName" } } },
        { "type": "object", "additionalProperties": false, "required": ["Inline"],
          "properties": { "Inline": { "$ref": "#/$defs/PublicKey" } } }
      ]
    },
    "LaunchSpec": {
      "type": "object",
      "additionalProperties": false,
      "description": "Fully-resolved, provider-neutral input to Compute::instance_launch. Replaces LaunchRequest.",
      "required": ["name", "image", "size", "key", "tags"],
      "properties": {
        "name":             { "$ref": "#/$defs/InstanceName" },
        "image":            { "$ref": "#/$defs/ImageRef" },
        "size":             { "type": "string", "description": "Instance type / machine type, verbatim to the provider." },
        "key":              { "$ref": "#/$defs/KeyRef" },
        "placement":        { "anyOf": [ { "type": "null" }, { "$ref": "#/$defs/Placement" } ] },
        "firewall":         { "anyOf": [ { "type": "null" }, { "$ref": "#/$defs/FirewallRef" } ] },
        "market":           { "anyOf": [ { "type": "null" }, { "$ref": "#/$defs/Market" } ] },
        "tags":             { "type": "array", "items": { "$ref": "#/$defs/Tag" } },
        "user_data_base64": { "type": ["string", "null"], "description": "Cloud-init user-data (output of bootstrap::encode_user_data*) for providers that consume it; null when bootstrap is delivered another way, e.g. a Fly OCI image plus files-injected key." }
      }
    },
    "Capabilities": {
      "type": "object",
      "additionalProperties": false,
      "description": "Runtime view of which optional capability traits an adapter implements. Derived from the Compute capability accessors plus supports_spot().",
      "required": ["key_registry", "image_resolver", "server_templates", "network_defaults", "spot"],
      "properties": {
        "key_registry":    { "type": "boolean" },
        "image_resolver":  { "type": "boolean" },
        "server_templates":{ "type": "boolean" },
        "network_defaults":{ "type": "boolean" },
        "spot":            { "type": "boolean" }
      }
    },
    "Instance": {
      "$comment": "Modified: description generalised from 'EC2 instance' to 'managed cloud VM'; shape unchanged.",
      "type": "object",
      "description": "A managed cloud VM, mapped at the adapter boundary from the provider's native instance type.",
      "additionalProperties": false,
      "required": ["id", "name", "state", "public_dns", "public_ip", "tags"],
      "properties": {
        "id":         { "$ref": "#/$defs/InstanceId" },
        "name":       { "anyOf": [ { "type": "null" }, { "$ref": "#/$defs/InstanceName" } ] },
        "state":      { "$ref": "#/$defs/InstanceState" },
        "public_dns": { "type": ["string", "null"] },
        "public_ip":  { "type": ["string", "null"] },
        "tags":       { "type": "array", "items": { "$ref": "#/$defs/Tag" } }
      }
    },
    "Error": {
      "$comment": "Modified: add CapabilityUnsupported; relax Adapter.provider from enum ['aws-ec2'] to any provider tag. Shown with only the changed/added members; merge into the existing Error oneOf.",
      "oneOf": [
        { "type": "object", "additionalProperties": false, "required": ["kind", "provider", "capability"],
          "properties": {
            "kind":       { "const": "CapabilityUnsupported" },
            "provider":   { "type": "string" },
            "capability": { "type": "string", "enum": ["spot", "server-templates", "network-defaults", "key-registry", "image-resolver"] }
          } },
        { "type": "object", "additionalProperties": false, "required": ["kind", "provider", "source"],
          "properties": {
            "kind":     { "const": "Adapter" },
            "provider": { "type": "string", "description": "Per-provider tag from Compute::provider(); no longer constrained to aws-ec2." },
            "source":   { "type": "string" }
          } }
      ]
    },
    "ExitCode": {
      "$comment": "Modified: add 9 (CapabilityUnsupported). 8 (CloudInitFailed) is already shipped.",
      "type": "integer",
      "enum": [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 70, 74, 130]
    }
  }
}
```

`MarketKind`, `SpotType`, `KeyName`, `PublicKey`, `Region`, `InstanceName`, `InstanceState`, and `Tag` are reused unchanged. `AmiId`, `SubnetId`, and `SecurityGroupId` remain in the schema but are annotated as AWS-adapter-internal (no longer referenced by any port type); a `$comment` on each records that.

---

## Implementation notes

Order matters: land the types and traits first so the AWS adapter and orchestration compile against them, then the registry, then tests.

```
1. Add provider-neutral newtypes: ImageRef + FirewallRef in crates/kleya-core/src/model/region.rs
   (or a new model/refs.rs), Placement in model/placement.rs, KeyRef + Market + LaunchSpec in
   model/launch.rs (replacing LaunchRequest). Keep AmiId/SubnetId/SecurityGroupId for the AWS crate.
2. Split crates/kleya-core/src/ports/cloud_compute.rs into:
     - ports/compute.rs       (trait Compute + capability accessors + supports_spot)
     - ports/capabilities.rs  (KeyRegistry, ImageResolver, ServerTemplates, NetworkDefaults)
   Update ports/mod.rs exports. Remove the old CloudCompute name (or re-export Compute as an alias
   during transition).
3. Add Error::CapabilityUnsupported in crates/kleya-core/src/error.rs; map to 9 in
   crates/kleya-cli/src/exit_code.rs. Stop hardcoding "aws-ec2" — Adapter.provider takes
   Compute::provider().
4. Rewrite LaunchService::run / build_plan in crates/kleya-core/src/commands/launch.rs to query
   capability accessors (image_resolver, key_registry, network_defaults, server_templates) and
   raise CapabilityUnsupported when config requests an absent one. Split ensure_keypair into the
   KeyRegistry path and the KeyRef::Inline path.
5. In crates/kleya-aws/src/ec2.rs, impl Compute for AwsEc2 (lifecycle + provider()="aws-ec2",
   supports_spot()=true) and impl KeyRegistry/ImageResolver/ServerTemplates/NetworkDefaults,
   returning Some(self) from each accessor. Move the existing method bodies into the matching
   capability impl; keep build_request_launch_template_data and the AmiId/SubnetId/SecurityGroupId
   mapping internal.
6. Make provider crates optional + feature-gated in crates/kleya-cli/Cargo.toml
   (kleya-aws = { optional = true }; [features] default = ["aws"], aws = ["dep:kleya-aws"]).
   Replace the match in crates/kleya-cli/src/dispatch.rs with provider_registry() whose arms are
   #[cfg(feature="…")]; an unselected/uncompiled provider → ConfigInvalid naming compiled features.
7. Update crates/kleya-core/src/test_support/in_memory_compute.rs to impl Compute + all four
   capabilities, and add a capability-absent fake (e.g. InMemoryComputeMinimal returning None from
   the accessors) so the CapabilityUnsupported and inline-key paths are testable.
8. Update the AWS Floci integration tests for the trait split (call sites move onto the capability
   accessors); the wire behaviour is unchanged.
```

References: [04-provider-port.md](../04-provider-port.md) (current trait), [06-launch-and-connect.md](../06-launch-and-connect.md) (current orchestration), and the thin-REST viability analysis that motivated the per-capability split (AWS keeps the SDK; Hetzner/Fly/DO/Vultr/Linode use bearer-token REST; GCP/Azure need only a token-acquisition crate).

---

## Merge plan

1. Apply each `Proposed changes` block to its canonical page; bump each touched page's `**Date:**` to the merge date.
2. Fold the `Type changes` `$defs` into `canonical-types.schema.json`: add `LaunchSpec`, `ImageRef`, `FirewallRef`, `Placement`, `Market`, `KeyRef`, `Capabilities`; replace the `Instance`, `Error`, and `ExitCode` defs with the modified forms; remove `LaunchRequest`; annotate `AmiId`/`SubnetId`/`SecurityGroupId` as adapter-internal.
3. No new canonical page is added; `04-provider-port.md` keeps its number with the split trait set described inline.
4. Flip this file's `**Status:**` to `Merged`, add `**Merged:** YYYY-MM-DD`, and move it to `docs/specs/changes/merged/`.
5. Update `docs/README.md`: remove this file from the pending Change-specs list.

---

## Assumptions and open questions

**Assumptions**

- The capability set (`KeyRegistry`, `ImageResolver`, `ServerTemplates`, `NetworkDefaults`, plus the `supports_spot` flag) covers the AWS-specific concepts that the current port bakes in. A future provider needing a concept outside these adds a new capability trait rather than widening `Compute`.
- Holding the port as `Arc<dyn Compute>` with `Option<&dyn Capability>` accessors stays object-safe under `async_trait`. Default-method accessors returning `None` keep adapters that lack a capability free of boilerplate.
- The cargo-feature-per-provider model composes with `cargo-dist`: the released binary builds with `default` features (`aws`), and alternate provider builds are a feature-flag matrix concern, not a code change.

**Decisions**

- *Capability accessors over a monolithic trait.* **A thin provider implements `Compute` plus only the capabilities its API has, instead of stubbing a dozen meaningless methods.** Returning `Option<&dyn Capability>` keeps the port object-safe and lets orchestration ask "can this provider do X?" at runtime, raising `CapabilityUnsupported` only when config actually requested X.
- *Fully-resolved `LaunchSpec`, not a template-name indirection.* **The AWS launch-template round-trip is an AWS optimisation, not a universal concept.** Resolving image/placement/firewall/key in `kleya-core` before calling `instance_launch` means a thin provider just maps the spec onto one HTTP POST; AWS still materialises a server-side template via the `ServerTemplates` capability.
- *`KeyRef::Registered | Inline`.* **Providers split on whether they keep a cloud-side key registry.** AWS imports and references a named key; providers that take the public key at launch use `Inline` and skip the registry round-trip and fingerprint check. The variant is chosen by `kleya-core` orchestration from whether `key_registry()` is `Some`, so an adapter may assume it receives the matching variant — a `KeyRef::Registered` reaching a no-registry adapter (e.g. Fly) is an orchestration bug, not a runtime input.
- *`LaunchSpec.user_data_base64` is optional.* **Cloud-init user-data is an AWS-shaped assumption, not a universal one.** Fly has no cloud-init; it bakes the toolchain into an OCI image and injects the SSH key via the machine `files` field, so its `LaunchSpec` carries `None`. Making the field `Option` rather than adding a "bootstrap-delivery" capability keeps `Compute` small; how the bootstrap actually reaches the box is an adapter concern. Surfaced by the Fly change ([2026-05-21-fly_provider.md](2026-05-21-fly_provider.md)); see [05-bootstrap-rendering.md](../05-bootstrap-rendering.md).
- *`Placement` carries zone/subnet only, not region.* **Region pins the adapter's client/endpoint at construction (AWS `default_region`, Fly `[providers.fly].region`); it is not a per-launch placement field.** Keeping it out also avoids forcing the AWS-shaped `Region` newtype onto provider-neutral placement, which the Fly change exposed — Fly's 3-letter region codes (`lhr`) do not match the `Region` regex.
- *Feature-gate adapter crates, keep a closed `Provider` enum.* **Binary bloat comes from linking SDKs, not from core enum variants.** Gating the optional adapter *crates* behind cargo features is what keeps `aws-sdk-*` out of a non-AWS build; the `#[non_exhaustive]` `Provider` enum can keep growing without recompiling being the problem.
- *`Adapter.provider` carries `Compute::provider()`.* **With more than one adapter the public error must name which provider failed.** This resolves the deferred open question in [07-error-model.md](../07-error-model.md).

**Open questions**

- *Provider value validation moving to the registry.* The `provider` config field is still validated by the closed `Provider::parse`. Should an unknown value fail at config-parse time (core) or at dispatch time (registry, where feature availability is known)? This change keeps core validation and adds a dispatch-time "compiled without feature X" error; revisit if the two error sites confuse operators. The Fly change ([2026-05-21-fly_provider.md](2026-05-21-fly_provider.md)) is the first second consumer — it adds `Provider::Fly` + `parse("fly")` and a `#[cfg(feature = "fly")]` registry arm — and confirms the core-validates / registry-gates split works; still open whether to consolidate the two error sites. Affects [03-configuration.md](../03-configuration.md) if changed.
- *Per-provider config sections.* Resolved by the Fly change ([2026-05-21-fly_provider.md](2026-05-21-fly_provider.md)): it adds an optional `providers` block (`ProvidersCfg`) on `Config`, with `[providers.fly]` (`FlyProviderCfg`: token-env, org, region, image, guest) as the first member, required only when that provider is selected. Later adapters add sibling members; the top-level AWS-shaped fields are ignored under a non-AWS provider. Affects [03-configuration.md](../03-configuration.md).
- *`Instance.id` regex relaxation.* Resolved: relax `InstanceId` to a provider-agnostic non-empty bound and have each adapter re-assert its own id format at its boundary (the AWS adapter keeps `^i-[0-9a-f]{8,32}$`). Both child changes rely on this — the Fly change stores raw Fly machine ids in `InstanceId`. The provider-discriminant alternative is rejected as heavier for no gain.
- *`kleya template` commands on template-less providers.* Fly ([2026-05-21-fly_provider.md](2026-05-21-fly_provider.md)) is the first such provider. The *launch* path is resolved — `kleya-core` expands the spec inline when `server_templates()` is `None`, so launching on Fly needs no server-side template. The explicit `kleya template create/list/...` subcommands ([02-cli-surface.md](../02-cli-surface.md)) against a no-`ServerTemplates` provider still need a defined UX — raise `CapabilityUnsupported` (exit 9) versus hide the subcommand — which the Fly change defers rather than settling.
- *Exit-code 8 in the schema.* The `ExitCode` enum gains 8 (`CloudInitFailed`, already shipped) alongside the new 9. If 8 was genuinely never added to the schema, this is also a latent divergence fix; confirm at merge.
