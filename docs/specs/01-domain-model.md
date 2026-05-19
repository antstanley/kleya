# 01 — Domain Model

**Status:** Implemented · **Date:** 2026-05-19 · **Owner:** Ant Stanley

`kleya-core::model` holds the workspace's domain types. Every type that crosses a public boundary is a newtype whose constructor validates inputs against a regex, length cap, or both — invalid states are unrepresentable.

See [`canonical-types.schema.json`](canonical-types.schema.json) for the JSON Schema mirror of every entity below.

---

## Validation discipline

- Each newtype wraps a `String` (or in some cases a fixed-size byte array) and exposes `pub fn new(raw) -> Result<Self>` plus `pub fn as_str(&self) -> &str`.
- Limits referenced in constructors are named constants in `kleya_core::limits` — there are no magic numbers in `model/`.
- Regexes are compiled once via `once_cell::sync::Lazy` and have the explanatory `expect("static regex compiles")` allowance — a single `#[allow(clippy::expect_used)]` per file.
- Every newtype has a test for at least one positive input, one negative input, and (where a length cap exists) the boundary triple `len-1`, `len`, `len+1`.

---

## Identifiers and names

| Newtype | Regex | Max bytes | Source |
|---|---|---|---|
| `InstanceName` | `^[a-z0-9][a-z0-9-]{0,62}$` | `INSTANCE_NAME_BYTES_MAX = 63` | [model/instance.rs](../../crates/kleya-core/src/model/instance.rs) |
| `InstanceId` | `^i-[0-9a-f]{8,32}$` | n/a | [model/instance.rs](../../crates/kleya-core/src/model/instance.rs) |
| `KeyName` | `^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,127}$` | `KEY_NAME_BYTES_MAX = 128` | [model/key.rs](../../crates/kleya-core/src/model/key.rs) |
| `Region` | `^[a-z]{2}-[a-z]+-[0-9]+$` | n/a | [model/region.rs](../../crates/kleya-core/src/model/region.rs) |
| Tag key | non-empty | `TAG_KEY_BYTES_MAX = 128` | [model/tag.rs](../../crates/kleya-core/src/model/tag.rs) |
| Tag value | (no minimum) | `TAG_VALUE_BYTES_MAX = 256` | [model/tag.rs](../../crates/kleya-core/src/model/tag.rs) |

`KeyName`'s leading-character class excludes `.` so `KeyName + ".pem"` cannot resolve to a hidden file or a path-traversal segment. The trailing class allows `.` for versions like `kleya.v2`. The intersection of EC2 key-pair naming rules and POSIX-portable filename characters drives the regex; reject anything else at the boundary so `<KeyName>.pem` is always safe to write.

---

## Entities

### `Instance` ([model/instance.rs](../../crates/kleya-core/src/model/instance.rs))

Represents a managed EC2 instance, mapped at the adapter boundary from `aws_sdk_ec2::types::Instance`.

- `id: InstanceId`
- `name: Option<InstanceName>` (from the `Name` tag)
- `state: InstanceState` — `Pending | Running | ShuttingDown | Terminated | Stopping | Stopped | Other(String)`
- `public_dns: Option<String>`, `public_ip: Option<String>`
- `tags: Vec<Tag>` — the full tag set, including the four `kleya:*` management tags

`InstanceFilter` carries the optional `name` filter, a `managed_only: bool`, and a `states: Vec<InstanceState>` shortlist; the AWS adapter translates these into `describe-instances` filters.

### `KeyName` / `KeyPair` / `PublicKey` / `Fingerprint` ([model/key.rs](../../crates/kleya-core/src/model/key.rs))

- `KeyName` — validated newtype (see regex above).
- `PublicKey(pub String)` — OpenSSH wire-format text of the public half (`ssh-ed25519 AAAA…`).
- `KeyPair { name: KeyName, public: PublicKey, private: String }` — only returned by `KeyStore::generate`; the `private` field is the OpenSSH-PEM body that was just written to disk at mode `0o600`.
- `Fingerprint(pub String)` — EC2-style **MD5 of the DER-encoded SubjectPublicKeyInfo** of the public key, formatted as colon-separated lowercase hex (`aa:bb:cc:…`, 47 chars). The local `KeyStore::fingerprint` impl computes this from the stored public half so it compares equal to what AWS returns from `DescribeKeyPairs` for an `ImportKeyPair`-imported key.

### `TemplateSpec` and friends ([model/template.rs](../../crates/kleya-core/src/model/template.rs))

A launch-template description, used both for `ensure_default_template` and for explicit `template create / update`:

- `name: TemplateName(String)` (the human handle)
- `ami_id: Option<AmiId>` and `ami_alias: Option<String>` — at least one must resolve at launch time
- `instance_type: String` — passed verbatim to AWS (e.g., `m8g.xlarge`)
- `key_name: KeyName`
- `security_group_ids: Vec<SecurityGroupId>` — empty means "let AWS apply the VPC default SG"; the adapter omits the field rather than passing `Some(vec![])`
- `subnet_id: Option<SubnetId>`
- `market: MarketKind { Spot | OnDemand }`, `spot_type: SpotType { OneTime | Persistent }`
- `tags: Vec<Tag>` — `Project=kleya` is always prepended; the four management tags are added at `instance_launch` time, not on the template
- `user_data_base64: String` — output of [`bootstrap::encode`](../../crates/kleya-core/src/bootstrap/encode.rs); see [05-bootstrap-rendering.md](05-bootstrap-rendering.md) for the encoding pipeline

`TemplateId(String)` and `TemplateVersion(u64)` are opaque AWS handles; `TemplateSummary` bundles `(id, name, latest_version)`.

### `LaunchRequest` ([model/launch.rs](../../crates/kleya-core/src/model/launch.rs))

The runtime invocation shape, distinct from `TemplateSpec` which describes the durable template:

- `template: TemplateName`
- `instance_name: InstanceName` — printed at launch and used as the `Name` tag
- `instance_type_override`, `market_override`, `spot_type_override` — `Option`s that, when `Some`, beat the template's values via the SDK's per-`RunInstances` overrides
- `extra_tags: Vec<Tag>` — appended to the four management tags
- `key_name: KeyName` — copied into the `kleya:key` tag

`Deadline { timeout, poll_interval, cancel: Option<CancellationToken> }` is threaded through `instance_wait_running` so the loop observes SIGINT without waiting for the next poll. See [06-launch-and-connect.md](06-launch-and-connect.md).

### `Tag` ([model/tag.rs](../../crates/kleya-core/src/model/tag.rs))

A `(String, String)` pair with length validation. The four management tag keys are constants:

- `KLEYA_TAG_NAME = "Name"` — human handle
- `KLEYA_TAG_MANAGED = "kleya:managed"` — value `"true"` for instances kleya owns
- `KLEYA_TAG_TEMPLATE = "kleya:template"` — template name the instance was launched from
- `KLEYA_TAG_KEY = "kleya:key"` — `KeyName` of the private key that opens this instance

**Tag-match semantics.** When filtering by `kleya:managed`, the match is on **key AND value** (`tag:kleya:managed=true`). A tag with key `kleya:managed` and any value other than `"true"` is not a managed instance.

---

## Relationships

```
TemplateSpec ─┐
              ├── (referenced by name) ── LaunchRequest ── (creates) ── Instance
KeyName ──────┘                                                          │
                                                                         ├── tags
                                                                         │     ├── Name
                                                                         │     ├── kleya:managed=true
                                                                         │     ├── kleya:template=<TemplateName>
                                                                         │     └── kleya:key=<KeyName>
                                                                         │
KeyStore (fs) ──── fingerprint ─── must equal ─── CloudCompute::keypair_fingerprint
                                                  (EC2 DescribeKeyPairs)
```

A managed instance carries its own metadata: `connect` resolves the right private key by reading `kleya:key` off the instance, with `keys.default_key_name` as the fallback only when the instance is also tagged `kleya:managed=true`. There is no local state file — the instance is the source of truth.

---

## Lifecycle: keypair

Evaluated on every launch by `LaunchService::ensure_keypair` (no first-run short-circuit, so out-of-band drift surfaces at the cost of one `DescribeKeyPairs` round-trip per launch).

```
         ┌───────────────────────────────────────────────────────────┐
         │             (KeyStore::exists, EC2 fingerprint)           │
         └────────────┬───────────────┬───────────────┬──────────────┘
                      │               │               │
              (true, Some)      (true, None)     (false, Some)    (false, None)
                      │               │               │               │
   fingerprints match?      ImportKeyPair      Error::KeyOrphaned   generate Ed25519
        │       │             (re-register)                          → write 0600
       yes      no                                                   → ImportKeyPair
        │       │
        ▼       ▼
       OK   Error::KeyMismatch
```

Fingerprints compared are the EC2-style MD5-of-DER-SPKI value (47 chars, colon-separated lowercase hex). The adapter additionally issues a follow-up `DescribeKeyPairs` after `ImportKeyPair` to guard the narrow TOCTOU window where AWS reports `InvalidKeyPair.Duplicate` for a key that was deleted between cache invalidation and the call.

## Lifecycle: instance

Mapped at the adapter boundary from EC2 instance state names.

```
   Pending ── RunInstances ──▶ Running ── TerminateInstances ──▶ ShuttingDown ──▶ Terminated
                                  │
                                  ├── Stopping ──▶ Stopped (not currently used by kleya)
                                  │
                                  └── Other(String) (unrecognised raw EC2 state)
```

`instance_wait_running` polls `Describe*` on `poll_interval` (default 5 s) until `state == Running` or `timeout` (default 600 s) is exceeded. A cancelled `CancellationToken` aborts the sleep immediately and returns `Error::Cancelled { instance }` rather than `Error::LaunchWaitTimeout`.

---

## Required query patterns

| Query | Access path | Where used |
|---|---|---|
| Find managed instance by `Name` tag | `instance_list(InstanceFilter { name: Some(handle), managed_only: true, .. })` | `connect`, `terminate` |
| Resolve `i-…` directly | `instance_describe(InstanceId)` | `connect`, `terminate` (any handle starting with `i-`) |
| Confirm a key pair exists | `keypair_fingerprint(KeyName) -> Option<Fingerprint>` | `ensure_keypair` (every launch) |
| Find a template by name | `template_get_by_name(TemplateName) -> Option<TemplateSummary>` | `ensure_template`, `template update` |
| Lookup default VPC subnet | `resolve_default_subnet() -> SubnetId` | `ensure_template` (deterministic pick: lexicographically first by AZ) |
| Resolve AL2023 AMI by alias | `resolve_ami_alias(&str) -> AmiId` | `build_plan` (`amazon-linux-2023-{arm64,x86_64}`) |

The handle-resolution outcomes (`InstanceNotFound`, single hit, `AmbiguousHandle`) are spelled out in [06-launch-and-connect.md](06-launch-and-connect.md).

---

## Assumptions and open questions

**Assumptions**

- AWS resource IDs match their documented regexes (`i-…`, `sg-…`, `subnet-…`, `ami-…`). The adapter does not re-validate IDs returned by the SDK.
- An instance's `kleya:key` tag value, if present, is itself a valid `KeyName` — written by `instance_launch` from a validated `KeyName`.
- Tag keys and values fit `TAG_KEY_BYTES_MAX` / `TAG_VALUE_BYTES_MAX`. EC2's own limits (128 / 256) are equal to these caps, so anything EC2 accepts on a `Describe*` round-trip is also accepted by the `Tag::new` validator.

**Decisions**

- *Newtypes wrap `String`, not `Vec<u8>` or `[u8; N]`.* **Simpler ownership; UTF-8 boundary at the constructor.** Every public path through `kleya-core` already requires UTF-8 (config files, regexes, tag values), so a `String` newtype is the smallest representation that carries the invariant.
- *Single `KeyName` regex spans EC2 + filesystem constraints.* **The intersection is small enough that two regexes would diverge.** A future GCP / Hetzner adapter that has stricter naming will validate again at its boundary; `kleya-core` keeps one rule.
- *`Fingerprint` is opaque from the type system's perspective.* **No structural validation on the inner string — equality is the only operation needed.** The format invariant (47-char colon-separated MD5 hex) is asserted at construction in `FsKeyStore::fingerprint`; the type itself is a transparent wrapper.

**Open questions**

- *Stop / restart workflow.* Resolved: `kleya start` and `kleya stop` are in scope. The CLI surface adds both commands; the call propagates to the provider crate which performs the stop/start against the underlying compute API.
- *Tag schema versioning.* Deferred: no v2 tag scheme is planned. Revisit if any `kleya:*` tag key ever changes; today old instances would be terminated and re-launched rather than upgraded in place.
