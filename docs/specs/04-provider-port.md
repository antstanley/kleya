# 04 — Provider Port

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya-core` describes the cloud provider as two `async_trait` ports — `CloudCompute` for compute resources and `KeyStore` for local key material. The AWS implementation lives in `kleya-aws`; the in-memory fakes used by the command tests live behind a `test-support` feature in `kleya-core`. `kleya-core` never imports `aws-sdk-*` directly.

---

## Responsibilities

1. Define a provider-neutral surface for every external operation `kleya` performs: compute lifecycle, default-resource lifecycle, AMI resolution, key pair imports, subnet lookup.
2. Make idempotency a contract rather than a hint — `ensure_default_*` methods must succeed when called repeatedly with the same arguments.
3. Translate provider-specific errors into `kleya_core::Error::Adapter { provider, source }` at the public boundary so `kleya-core` never sees `aws_sdk_ec2::Error`.

---

## `CloudCompute` trait

[crates/kleya-core/src/ports/cloud_compute.rs](../../crates/kleya-core/src/ports/cloud_compute.rs).

```rust
#[async_trait]
pub trait CloudCompute: Send + Sync {
    // Launch templates
    async fn template_create(&self, spec: &TemplateSpec) -> Result<TemplateId>;
    async fn template_update(&self, id: &TemplateId, spec: &TemplateSpec) -> Result<TemplateVersion>;
    async fn template_list(&self) -> Result<Vec<TemplateSummary>>;
    async fn template_delete(&self, id: &TemplateId) -> Result<()>;
    async fn template_get_by_name(&self, name: &TemplateName) -> Result<Option<TemplateSummary>>;

    // Instances
    async fn instance_launch(&self, req: &LaunchRequest) -> Result<Instance>;
    async fn instance_list(&self, filter: &InstanceFilter) -> Result<Vec<Instance>>;
    async fn instance_describe(&self, id: &InstanceId) -> Result<Instance>;
    async fn instance_terminate(&self, id: &InstanceId) -> Result<()>;
    async fn instance_wait_running(&self, id: &InstanceId, deadline: Deadline) -> Result<Instance>;

    // Default-resource lifecycle (idempotent)
    async fn ensure_default_security_group(&self, name: &str) -> Result<SecurityGroupId>;
    async fn ensure_default_keypair(&self, name: &KeyName, public_key: &PublicKey) -> Result<()>;
    async fn ensure_default_template(&self, spec: &TemplateSpec) -> Result<TemplateId>;

    // Resolution helpers
    async fn keypair_fingerprint(&self, name: &KeyName) -> Result<Option<Fingerprint>>;
    async fn resolve_default_subnet(&self) -> Result<SubnetId>;
    async fn resolve_ami_alias(&self, alias: &str) -> Result<AmiId>;
}
```

The trait is split into four functional groups: durable template CRUD, per-instance lifecycle, idempotent defaults, and read-only resolvers. Each group has a single concern; methods do not cross-cut.

### Idempotency contract

All `ensure_default_*` methods must treat provider-side "already exists" responses (`InvalidGroup.Duplicate`, `InvalidKeyPair.Duplicate`, …) as success **after a follow-up `Describe*` confirms the existing resource matches**. The adapter handles this; `kleya-core` orchestration does not retry.

- `ensure_default_security_group(name)` — find by name; if absent create; either way authorize the default ingress rule (`22/tcp` from `0.0.0.0/0`), treating `InvalidPermission.Duplicate` as success. Returns the `SecurityGroupId`.
- `ensure_default_keypair(name, public_key)` — `describe_key_pairs` first; if absent, `import_key_pair`, treating `InvalidKeyPair.Duplicate` as success. **Then unconditionally re-describe** to confirm the key actually exists in EC2 — guards the narrow TOCTOU window where AWS reports `Duplicate` for a key that was deleted between cache invalidation and the call. Without the confirm, a later `RunInstances` would fail with a cryptic `KeyPair does not exist`.
- `ensure_default_template(spec)` — `template_get_by_name` first; if `Some`, return that id; otherwise `template_create`. Note that this never *updates* an existing template; differing `spec` against an existing same-named template is a no-op. Explicit `template update` is the path for that.

### Tag-match semantics

`instance_list(InstanceFilter { managed_only: true, .. })` matches **key AND value** — `tag:kleya:managed=true`. A tag with key `kleya:managed` and any other value is not a managed instance. The AWS adapter translates this into a `tag:kleya:managed=true` describe-instances filter.

### Deterministic resolution

`resolve_default_subnet()` filters VPCs by `isDefault=true`, then picks the subnet whose `availability_zone` is **lexicographically first** across the default VPC's subnets. This makes the zero-config launch deterministic across runs and across operators with the same default VPC layout.

`resolve_ami_alias(alias)` translates a stable shorthand to an SSM-public-parameter lookup and returns the resolved AMI id at launch time. Today two aliases are supported:

| Alias | SSM parameter |
|---|---|
| `amazon-linux-2023-arm64` | `/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64` |
| `amazon-linux-2023-x86_64` | `/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64` |

Any other alias returns `Error::ConfigInvalid`.

---

## `KeyStore` trait

[crates/kleya-core/src/ports/key_store.rs](../../crates/kleya-core/src/ports/key_store.rs).

```rust
pub trait KeyStore: Send + Sync {
    fn ensure_dir(&self) -> Result<PathBuf>;
    fn generate(&self, name: &KeyName) -> Result<KeyPair>;
    fn read_public(&self, name: &KeyName) -> Result<PublicKey>;
    fn private_path(&self, name: &KeyName) -> Result<PathBuf>;
    fn exists(&self, name: &KeyName) -> bool;
    fn delete(&self, name: &KeyName) -> Result<()>;

    /// EC2-style MD5 of the DER-encoded SPKI of the public key, colon-separated
    /// lowercase hex. Must equal what AWS returns from `DescribeKeyPairs`.
    fn fingerprint(&self, name: &KeyName) -> Result<Fingerprint>;
}
```

The only implementation is `FsKeyStore` in [crates/kleya-cli/src/key_store_fs.rs](../../crates/kleya-cli/src/key_store_fs.rs):

- Directory permissions: `0o700`, asserted at every operation against `keys.dir`.
- File permissions: `0o600`, asserted on read and after write.
- Key algorithm: **Ed25519** via the `ssh-key` crate; private half written in OpenSSH format.
- Fingerprint: MD5 over the manually-constructed DER SPKI bytes for Ed25519 (`30 2A 30 05 06 03 2B 65 70 03 21 00 || <32-byte pubkey>` → MD5 → colon-separated hex). The SPKI construction is a single 44-byte buffer with `assert_eq!(der.len(), 44)`; the output is `assert_eq!(out.len(), 47)`.

**Important.** It is *not* correct to MD5 the OpenSSH wire-format body (the bytes between `ssh-ed25519 ` and the comment) — those bytes differ from the DER SPKI and would not match `DescribeKeyPairs`. The handcrafted DER prefix is the load-bearing detail here.

---

## Supporting ports

### `Clock` ([ports/clock.rs](../../crates/kleya-core/src/ports/clock.rs))

```rust
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, dur: Duration);
}
```

`SystemClock` is the production impl. Tests use the tokio runtime's paused-clock harness (`#[tokio::test(start_paused = true)]`) plus a `FakeClock` from `test_support` for the few sync paths.

### `IdGen` ([ports/id_gen.rs](../../crates/kleya-core/src/ports/id_gen.rs))

```rust
pub trait IdGen: Send + Sync {
    fn name(&self) -> String;
}
```

Production impl is `AdjAnimalIdGen` — two static word lists indexed by `SystemTime::now().duration_since(UNIX_EPOCH).as_nanos()` (and `nanos / 7` for the noun). Produces names like `kleya-brave-otter`, satisfying the `InstanceName` regex by construction. Tests use `FakeIdGen` with a fixed sequence.

---

## AWS adapter — `AwsEc2`

[crates/kleya-aws/src/ec2.rs](../../crates/kleya-aws/src/ec2.rs). Implements `CloudCompute` against `aws-sdk-ec2` plus `aws-sdk-ssm` (for the AMI alias resolver). Three deliberate design points:

1. **Client-injection.** The `AwsEc2` struct holds `Arc<Ec2Client>` and `Arc<SsmClient>` so test code in `kleya-aws` can build them against a Floci emulator endpoint via `client::build_ec2_client(region, endpoint_url)`. Production calls pass `None`; tests pass `Some("http://localhost:4566")` plus static `test`/`test` credentials. `kleya-core` never sees this knob.
2. **Builder mapping is centralised in `build_request_launch_template_data`.** This is the one place `TemplateSpec` becomes a `RequestLaunchTemplateData`. Per-field rules: empty `security_group_ids` is omitted entirely (rather than `Some(vec![])`) so AWS applies the VPC default SG; `MarketKind::OnDemand` omits the market-options field; `ami_id` is only set when explicitly supplied.
3. **Error surface.** `AwsError::Sdk` wraps a `BoxError`, `AwsError::MissingField` wraps a static `&'static str`, `AwsError::SsmMissing` wraps an SSM parameter name. The `From<AwsError> for kleya_core::Error` impl calls `Error::adapter("aws-ec2", e)`, so the public error always carries the provider tag.

### Cancellation in poll loops

`instance_wait_running` and the CLI's `ssh_probe::probe_ssh_ready` use a shared helper `kleya_core::util::wait_or_cancel(interval, cancel)` that does:

```rust
match &cancel {
    Some(c) => tokio::select! {
        () = c.cancelled() => return true,
        () = tokio::time::sleep(interval) => return false,
    },
    None => { tokio::time::sleep(interval).await; false }
}
```

Callers observe a `true` return as cancellation and short-circuit with `Error::Cancelled { instance }`. There is no separate top-of-loop `is_cancelled()` check — `wait_or_cancel` is the single point that sees the token.

### `RunInstances` tagging

Every instance launched by the AWS adapter is tagged with at minimum:

```
Name             = <instance_name>
kleya:managed    = "true"
kleya:template   = <template_name>
kleya:key        = <key_name>
```

Plus any tags from `TemplateSpec.tags` (which already include `Project=kleya` and the operator-defined `[[templates.tags]]` block from config). The `kleya:template` and `kleya:key` tags are what lets `connect` resolve the right private key path without a local state file.

---

## In-memory fakes (`test_support`)

Feature-gated under `test-support` in `kleya-core` ([test_support/mod.rs](../../crates/kleya-core/src/test_support/mod.rs)):

- `InMemoryCompute` — a `Mutex<HashMap<…>>` per resource type implementing `CloudCompute`. Used by command-level integration tests.
- `InMemoryKeyStore` — analogous for `KeyStore`. Bypasses Unix mode-bit assertions.
- `FakeClock`, `FakeIdGen` — deterministic time and name generation.

The feature is enabled by `dev-dependencies` automatically when `kleya-core` is built as a test target. Per-crate test runs (`cargo nextest run -p kleya-core`) need the feature flag explicitly; the workspace test run does not.

---

## Assumptions and open questions

**Assumptions**

- AWS SDK error metadata exposes `code()` on every error variant we filter on (`InvalidGroup.Duplicate`, `InvalidKeyPair.Duplicate`, `InvalidPermission.Duplicate`). The `aws-sdk-ec2` impl currently satisfies this through `ProvideErrorMetadata`.
- SSM `GetParameter` for the documented AL2023 public parameters does not require elevated IAM beyond the `ssm:GetParameter` action on the `aws:*` namespace.
- Floci behaves like real EC2 for the operations the integration tests exercise. The one known divergence (`CreateLaunchTemplate` unsupported by Floci today) is documented in [CONTRIBUTING.md](../../CONTRIBUTING.md) and surfaced as `continue-on-error: true` in CI.

**Decisions**

- *Idempotency is a port contract, not an adapter convenience.* **Every `ensure_default_*` must succeed under concurrent invocations.** `kleya-core` orchestration is straight-line — no retry, no compensation. Putting the duplicate-handling in the adapter makes each call site simple and gives every future provider the same contract.
- *Confirm-after-import on key pairs.* **Unconditional follow-up `DescribeKeyPairs` after a successful or duplicate `ImportKeyPair`.** Cheap (~50 ms) insurance against a real TOCTOU we hit during development; not worth dropping.
- *Deterministic subnet pick by lexicographic AZ.* **Two operators with the same default VPC layout get the same subnet.** Alternative (alphabetical-by-subnet-id) ties launches to opaque AWS-generated ids that change across regions; AZ names are stable.
- *Empty `security_group_ids` omits the field.* **`Some(vec![])` would tell EC2 "explicitly no SGs", which is an error.** Treat empty as "use the VPC default" and let AWS apply it.
- *MD5-of-DER-SPKI fingerprint, not OpenSSH wire-format.* **EC2 returns SPKI MD5 for imported keys.** The OpenSSH wire-format MD5 is a tempting but wrong shortcut that would cause every fingerprint comparison to mismatch.

**Open questions**

- *Pagination caps.* `instance_list` and `template_list` walk SDK paginators without an explicit page-count ceiling. A pathological account with 10k+ launch templates would issue many calls; revisit if a real account hits this.
- *Region-pinned clients.* `AwsEc2` is single-region today. Multi-region orchestration is non-goal #2 in [00-overview.md](00-overview.md); when a second adapter is added, decide whether per-call region overrides become a `CloudCompute` parameter or a per-`CloudCompute`-instance setting.
- *Non-AWS adapter parity.* The `keypair_fingerprint` contract (EC2 MD5-of-DER-SPKI) is provider-specific. A future GCP or Hetzner adapter will need its own fingerprint algorithm; whether the trait should expose `fn fingerprint_algorithm(&self) -> &str` so the local `KeyStore` can match it is unresolved.
