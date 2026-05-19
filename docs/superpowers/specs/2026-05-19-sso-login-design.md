# `kleya` — SSO Login Design (Withdrawn)

**Status:** Withdrawn · **Date:** 2026-05-19 · **Owner:** Ant Stanley

This draft proposed enabling the `credentials-login` Cargo feature on `aws-config` and adding a `kleya sso login | logout | status` subcommand tree so kleya could drive the IAM Identity Center device-authorization OIDC flow itself, removing the AWS CLI v2 dependency for SSO users.

**The proposal is withdrawn.** Operator decision recorded 2026-05-19: AWS authentication (and any future provider's authentication) must happen outside kleya. The CLI is to remain a pure consumer of whatever credentials the SDK's default chain resolves — `aws sso login`, `aws configure`, `aws-vault`, `granted`, and equivalent tools are the supported login paths. Quoting the operator: "logging into AWS (or another other provider) must happen outside of the CLI, the cli should just use those credentials."

Practical effect:

- The `credentials-login` Cargo feature on `aws-config` is **not** enabled. Default features (`sso`, `credentials-process`, `rt-tokio`, `default-https-client`) cover the supported flows via the SDK chain.
- No `kleya sso` subcommand tree is added.
- No `Error::SsoLoginFailed` variant or new exit code is added.
- No new dependencies (`aws-sdk-signin`, `p256`, `sha2`, `uuid`, `base64-simd`, `rand`) enter the binary.

The canonical record of how kleya consumes SSO (and every other AWS credential source) is [`docs/specs/11-credentials-and-sso.md`](../../specs/11-credentials-and-sso.md). That page describes the shipped consumption-only behaviour and records this withdrawal as a Decision.

This file is kept for history; subsequent reviewers may want to see what was proposed and why it was rejected before reopening the topic.
