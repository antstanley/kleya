# Change: AWS adapter — thin-REST client (drop the service SDKs)

**Status:** Proposed · **Date:** 2026-05-21 · **Owner:** Ant Stanley · **Target:** Repo-wide · **Depends on:** [2026-05-21-provider_neutral_port.md](2026-05-21-provider_neutral_port.md)

`kleya-aws` will stop depending on `aws-sdk-ec2` and `aws-sdk-ssm` and instead call the EC2 and SSM HTTP APIs directly with `reqwest`, signing each request with **`aws-sigv4`** and resolving credentials and region with **`aws-config`**. The capability-trait impls introduced by the parent change (`Compute`, `KeyRegistry`, `ImageResolver`, `ServerTemplates`, `NetworkDefaults` on `AwsEc2`) keep their public behaviour; only the transport beneath them changes — hand-built EC2 Query-protocol requests with XML response parsing, and SSM `AwsJson1_1` requests. Both `aws-config` (1.8.x) and `aws-sigv4` (1.4.x) are published, importable crates, so **no code is vendored**; the conditional vendoring the request anticipated does not apply.

---

## Motivation

The two AWS service SDKs are the dominant weight in a kleya build: `aws-sdk-ec2` alone is a multi-megabyte generated crate covering hundreds of operations, of which kleya calls about fifteen. The parent change makes the AWS adapter one of several behind a feature flag; this change shrinks that adapter to the operations kleya actually issues, so the `aws` feature stops dragging the full EC2/SSM code-gen into every default build.

The credential chain is the one part worth keeping from the AWS runtime. `aws-config` resolves env vars, shared profiles, SSO cached tokens, `aws login` console credentials, web-identity, and IMDS — reimplementing that is more work and more risk than the signing or the request bodies, and [11-credentials-and-sso.md](../11-credentials-and-sso.md) already documents kleya's reliance on it. So credentials stay on `aws-config`, request signing moves to the standalone `aws-sigv4` crate, and transport moves to `reqwest`. The net binary saving is real but partial: dropping `aws-sdk-ec2` + `aws-sdk-ssm` removes the service code-gen, while `aws-config` still pulls `aws-smithy-runtime`, an HTTP/TLS stack, and `aws-sdk-sts` (plus optional `aws-sdk-sso`/`aws-sdk-ssooidc`) for the credential providers.

---

## Affected spec pages

| Canonical page | Nature of change |
|---|---|
| [`docs/specs/04-provider-port.md`](../04-provider-port.md) | Rewrite the "AWS adapter — `AwsEc2`" section: thin-REST client (reqwest + aws-sigv4 + aws-config + quick-xml) replaces the SDK clients; capability impls unchanged |
| [`docs/specs/07-error-model.md`](../07-error-model.md) | Replace `AwsError::Sdk` with transport/signing/decode/api variants; duplicate-code detection now parses the EC2 error XML |
| [`docs/specs/09-architecture-principles.md`](../09-architecture-principles.md) | Dependency graph and `kleya-aws` module table: drop `aws-sdk-*`, add `reqwest`/`aws-sigv4`/`quick-xml`, keep `aws-config` |

No canonical-schema change: `AwsError` is adapter-local (described only in prose in `07`) and no domain entity changes. The credential-sourcing prose in [11-credentials-and-sso.md](../11-credentials-and-sso.md) stays valid (the chain is still `aws-config`); its wording is re-checked at merge in case it implies the SDK performs transport.

---

## Proposed changes

### `docs/specs/04-provider-port.md` → AWS adapter — `AwsEc2` (Modify)

> ## AWS adapter — `AwsEc2`
>
> [crates/kleya-aws/src/ec2.rs](../../crates/kleya-aws/src/ec2.rs). `AwsEc2` implements `Compute` and all four capability traits (see the trait split above); the operation set and behaviour are unchanged. Beneath them, it issues HTTP directly rather than through `aws-sdk-ec2` / `aws-sdk-ssm`:
>
> 1. **Credentials and region — `aws-config`.** At construction, `client::build_aws_client(region, endpoint_url)` calls `aws_config::defaults(BehaviorVersion::latest()).region(...).load().await` to obtain an `SdkConfig`. The credential provider (`SdkConfig::credentials_provider()`, a `SharedCredentialsProvider`) is held and its `provide_credentials()` is awaited per request batch; resolved `Credentials` (access key, secret, optional session token, expiry) are cached until shortly before `expiry`. This preserves the full default chain documented in [11-credentials-and-sso.md](../11-credentials-and-sso.md) — SSO, `aws login`, profiles, web-identity, IMDS.
> 2. **Signing — `aws-sigv4`.** Each request is signed with `aws_sigv4`: a `SigningParams` is built from the cached credentials (as an `Identity`), the region, the service name (`"ec2"` or `"ssm"`), and the request time; `sign(SignableRequest{method, uri, headers, body}, &params)` returns `SigningInstructions` whose headers (`Authorization`, `X-Amz-Date`, and `X-Amz-Security-Token` when a session token is present) are applied to the outgoing `reqwest::Request`.
> 3. **Transport — `reqwest`.** EC2 calls `POST https://ec2.<region>.amazonaws.com/` with a form-encoded body in the **EC2 Query protocol** (`Action=RunInstances&Version=2016-11-15&...`) and parse the XML response with `quick-xml` + `serde`. SSM's `resolve_image` calls `POST https://ssm.<region>.amazonaws.com/` in the `AwsJson1_1` protocol (`X-Amz-Target: AmazonSSM.GetParameter`, JSON body and response). The Floci test seam is preserved: `build_aws_client(region, Some("http://localhost:4566"))` points `reqwest` at the emulator; signing still runs with the static `test`/`test` credentials Floci accepts.
>
> The EC2 operations behind each capability are: `RunInstances` / `DescribeInstances` / `TerminateInstances` (`Compute`); `ImportKeyPair` / `DescribeKeyPairs` / `DeleteKeyPair` (`KeyRegistry`); `CreateLaunchTemplate` / `CreateLaunchTemplateVersion` / `DescribeLaunchTemplates` / `DeleteLaunchTemplate` (`ServerTemplates`); `DescribeSecurityGroups` / `CreateSecurityGroup` / `AuthorizeSecurityGroupIngress` / `DescribeVpcs` / `DescribeSubnets` (`NetworkDefaults`); and SSM `GetParameter` (`ImageResolver`). The `RunInstances` tagging block, the `build_request_launch_template_data` field rules, the deterministic lexicographically-first-AZ subnet pick, and the cancellable poll loops are unchanged — they now operate over hand-built request bodies and parsed XML structs instead of SDK builders and SDK types.
>
> ### Idempotency duplicate-detection
>
> The `ensure_*` idempotency contract is unchanged, but the duplicate signal is read differently. An EC2 "already exists" response is HTTP 400 carrying `<Response><Errors><Error><Code>InvalidGroup.Duplicate</Code>…`. The adapter parses that `<Code>` from the error XML (replacing the former `ProvideErrorMetadata::code()` check) and treats `InvalidGroup.Duplicate` / `InvalidKeyPair.Duplicate` / `InvalidPermission.Duplicate` as success, then issues the confirming `Describe*` exactly as before.

### `docs/specs/07-error-model.md` → `kleya_aws::AwsError` (Modify)

> ## `kleya_aws::AwsError`
>
> [crates/kleya-aws/src/error.rs](../../crates/kleya-aws/src/error.rs).
>
> ```rust
> #[derive(Debug, thiserror::Error)]
> pub enum AwsError {
>     #[error("http transport: {0}")]
>     Http(#[from] reqwest::Error),
>     #[error("request signing: {0}")]
>     Signing(String),
>     #[error("decode {protocol} response: {message}")]
>     Decode { protocol: &'static str, message: String },
>     #[error("ec2/ssm api error {code}: {message}")]
>     Api { code: String, message: String },
>     #[error("missing field in response: {0}")]
>     MissingField(&'static str),
> }
>
> impl From<AwsError> for kleya_core::Error {
>     fn from(e: AwsError) -> Self {
>         kleya_core::Error::adapter("aws-ec2", e)
>     }
> }
> ```
>
> - **`Http`** — any `reqwest` transport failure (DNS, TLS, connect, timeout, body read).
> - **`Signing`** — an `aws-sigv4` signing error or a missing/expired credential resolution from `aws-config`, stringified.
> - **`Decode { protocol, message }`** — a malformed or unexpected EC2 XML (`protocol = "ec2-query"`) or SSM JSON (`protocol = "aws-json-1.1"`) response.
> - **`Api { code, message }`** — a structured EC2/SSM error parsed from the response (the EC2 `<Error><Code>/<Message>` or the SSM JSON `__type`/`message`). The duplicate-handling logic inspects `Api.code` *before* the value is wrapped, so a tolerated `*.Duplicate` never surfaces as `Error::Adapter`.
> - **`MissingField`** — a documented field absent from an otherwise-successful response; the static string names it.
>
> The `From<AwsError> for kleya_core::Error` impl still tags the public error `provider: "aws-ec2"`. (After the parent change `Adapter.provider` is a free-form per-provider tag; AWS continues to use `"aws-ec2"`.)

### `docs/specs/09-architecture-principles.md` → Dependency graph + `kleya-aws` conventions (Modify)

> The `kleya-aws` box no longer links the service SDKs. It depends on `kleya-core`, `aws-config` (credential chain + region), `aws-credential-types` (the `Credentials`/`Identity` bridge), `aws-sigv4` (request signing), `reqwest` (rustls TLS), `quick-xml` + `serde` (EC2 XML), and `serde_json` (SSM). `aws-sdk-ec2` and `aws-sdk-ssm` are removed. The provider-neutral rule is unchanged: `kleya-core` links none of these.
>
> ```
> ┌────────────────────┐
> │      kleya-aws     │  (feature "aws")
> │  → kleya-core      │
> │  → aws-config      │  credential chain + region (pulls aws-smithy-runtime,
> │  → aws-credential- │  HTTP/TLS, aws-sdk-sts, optional sso/ssooidc)
> │     types          │
> │  → aws-sigv4       │  request signing (standalone)
> │  → reqwest         │  transport
> │  → quick-xml/serde │  EC2 Query-protocol XML
> │  → serde_json      │  SSM AwsJson1_1
> └────────────────────┘
> ```
>
> `kleya-aws` module table:
>
> | Module | Role |
> |---|---|
> | `client` | `build_aws_client(region, endpoint_url)` — `aws-config` load, credential cache, the signed `reqwest::Client`, and the EC2/SSM endpoint resolution (incl. the Floci override) |
> | `sign` | Wraps `aws-sigv4`: build `SigningParams` from cached `Credentials`, sign a `reqwest::Request` for service `ec2`/`ssm` |
> | `ec2_query` | EC2 Query-protocol request builders (`Action=…&Version=2016-11-15`) and `quick-xml` response structs |
> | `ssm_json` | SSM `AwsJson1_1` request/response (`GetParameter`) |
> | `ec2` | `AwsEc2` struct + `Compute` and capability-trait impls over `ec2_query` / `ssm_json` |
> | `error` | `AwsError` enum + `From<AwsError> for kleya_core::Error` |
> | `tests/floci/` | Floci-backed integration tests, now driving the reqwest client at the emulator endpoint |

---

## Implementation notes

Order: stand up the signed client and one round-trip, then port operations capability by capability so the Floci tests stay green throughout.

```
1. crates/kleya-aws/Cargo.toml: remove aws-sdk-ec2, aws-sdk-ssm; add reqwest
   (default-features off, features ["rustls-tls","json"]), aws-sigv4, quick-xml
   (features ["serialize"]); keep aws-config, aws-credential-types, aws-smithy-types.
2. src/client.rs: replace build_ec2_client with build_aws_client(region, endpoint_url)
   — aws_config::defaults(BehaviorVersion::latest()).region(region).load(), hold
   SharedCredentialsProvider, build a reqwest::Client. Cache resolved Credentials with
   expiry; re-resolve when within a small skew of expiry.
3. src/sign.rs: given (&reqwest::Request parts, service, region, &Credentials), produce
   the SigV4 headers via aws_sigv4 and apply them. Add a known-answer unit test against
   the AWS SigV4 test-suite vectors so signing is verified without a network call.
4. src/ec2_query.rs: form-encode request builders for the 13 EC2 actions; quick-xml/serde
   structs for their responses and for the <Response><Errors><Error> error shape. Map the
   error <Code> to AwsError::Api { code, message }.
5. src/ssm_json.rs: GetParameter request/response (AwsJson1_1).
6. src/ec2.rs: re-point the Compute + capability impls at ec2_query/ssm_json. Keep
   build_request_launch_template_data field rules, RunInstances tagging, AZ subnet pick,
   wait_or_cancel loops. Move the *.Duplicate tolerance onto AwsError::Api.code.
7. src/error.rs: AwsError per the block above; delete the Sdk(BoxError) variant.
8. cargo deny check — confirm reqwest/quick-xml/aws-sigv4 and their trees pass licenses
   and advisories (deny.toml). reqwest must use rustls, not native-tls, to match policy.
9. tests/floci/*: point build_aws_client at http://localhost:4566; assert the signed
   requests round-trip. The EC2 Query/XML path is now exercised end-to-end by Floci.
```

References: `aws-sigv4` 1.4.x docs (`sign::v4`, `http_request::{SignableRequest, SigningSettings, sign}`); `aws-config` 1.8.x (`defaults`, `BehaviorVersion`, `SdkConfig::credentials_provider`); `aws-credential-types` (`ProvideCredentials`, `Credentials`, `Identity`); the [EC2 Query API reference](https://docs.aws.amazon.com/AWSEC2/latest/APIReference/) (version `2016-11-15`); the AWS SigV4 test-suite vectors for the signing unit test.

---

## Merge plan

1. Apply each `Proposed changes` block to its canonical page; bump each touched page's `**Date:**` to the merge date.
2. No `canonical-types.schema.json` change.
3. No new canonical page.
4. Flip this file's `**Status:**` to `Merged`, add `**Merged:** YYYY-MM-DD`, and move it to `docs/specs/changes/merged/`.
5. Update `docs/README.md`: remove this file from the pending Change-specs list.
6. Re-read [11-credentials-and-sso.md](../11-credentials-and-sso.md) and adjust only if any sentence implied the *SDK* (rather than `aws-config`) performed credential resolution or transport.

---

## Assumptions and open questions

**Assumptions**

- The parent change ([2026-05-21-provider_neutral_port.md](2026-05-21-provider_neutral_port.md)) has shipped: `AwsEc2` already implements `Compute` + the four capability traits, so this change only swaps the transport beneath them.
- `aws-config` 1.8.x and `aws-sigv4` 1.4.x are usable at the workspace MSRV (1.95.0) and pass `cargo deny` (both Apache-2.0). `aws-sigv4` is standalone, so no SDK service crate is reintroduced through it.
- Floci accepts SigV4-signed EC2 Query requests with static `test`/`test` credentials, exactly as it did for the SDK client. If Floci is signature-lax, signing is still applied; if it is signature-strict, the test credentials satisfy it.
- The fifteen EC2/SSM operations kleya issues are stable on EC2 API version `2016-11-15`; the Query protocol and its XML shapes are not changing.

**Decisions**

- *Import `aws-config` + `aws-sigv4`; vendor nothing.* **Both are published, and `aws-sigv4` is explicitly standalone (works without the SDK).** The request's "vendor if no crate exists" condition resolves to "import": vendoring would duplicate maintained, audited code for no benefit. If a future MSRV or license conflict forced it, `aws-sigv4` (Apache-2.0, self-contained) is the only realistic vendoring candidate, with the SigV4 test-suite vectors as its test floor.
- *Keep `aws-config` for credentials; replace only the service SDKs.* **The credential chain is the hard, high-value part; the EC2/SSM call surface kleya uses is small and stable.** Reimplementing SSO/STS/web-identity/IMDS resolution would be far more code and risk than hand-building fifteen request bodies.
- *EC2 Query protocol + `quick-xml`, SSM `AwsJson1_1` + `serde_json`.* **Match each service's native wire protocol rather than inventing one.** EC2's `2016-11-15` API is Query/XML; SSM is JSON — using the real protocols keeps the requests copy-checkable against the AWS API reference.
- *`reqwest` with `rustls`, not `native-tls`.* **Matches the existing TLS/supply-chain posture and avoids a system OpenSSL dependency** across the cargo-dist target matrix.

**Open questions**

- *Credential refresh granularity.* Resolving credentials once per CLI invocation is almost always enough (kleya runs seconds, not hours). Should the cache re-resolve mid-run only on a 403/expired-token response, or proactively near `expiry`? Start with per-invocation resolution plus a single retry on an expired-credential `Api` error; revisit if long `instance_wait_running` loops outlive a short-lived SSO token.
- *Pagination on `Describe*`.* The SDK paginators are gone; `DescribeInstances` / `DescribeLaunchTemplates` now need explicit `NextToken` loop handling in `ec2_query`. Bounded by the same "fine until 10k+ resources" envelope flagged in [04-provider-port.md](../04-provider-port.md); confirm the token loop has an attempt cap consistent with the no-unbounded-loops rule.
- *Binary-size target.* This removes the service SDKs but keeps the `aws-config` runtime tree. If the `aws` build is still heavier than wanted, a later change could swap `aws-config` for a hand-rolled subset of the credential chain (env + profile + IMDS only), trading SSO/`aws login` support for size — explicitly out of scope here.
