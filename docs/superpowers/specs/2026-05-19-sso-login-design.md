# `kleya` — SSO Login Design (Withdrawn)

**Status:** Withdrawn · **Date:** 2026-05-19 · **Owner:** Ant Stanley

This draft proposed enabling the `credentials-login` Cargo feature on `aws-config` and adding a `kleya sso login | logout | status` subcommand tree so kleya could drive the IAM Identity Center device-authorization OIDC flow itself, removing the AWS CLI v2 dependency for SSO users.

**The proposal is withdrawn.** Operator decision recorded 2026-05-19: AWS authentication (and any future provider's authentication) must happen outside kleya. The CLI is to remain a pure consumer of whatever credentials the SDK's default chain resolves — `aws sso login`, `aws configure`, `aws-vault`, `granted`, and equivalent tools are the supported login paths. Quoting the operator: "logging into AWS (or another other provider) must happen outside of the CLI, the cli should just use those credentials."

Practical effect:

- No `kleya sso` (or `kleya login`) subcommand tree is added — that surface is the part the operator explicitly rejected.
- No `Error::SsoLoginFailed` variant or new exit code is added; auth failures continue to surface as `Error::Adapter` (exit 70).

What this withdrawal does **not** decide:

- The `credentials-login` Cargo feature on `aws-config` **is enabled** (commit `a35b331b`) because the same feature also gates `aws_config::login::LoginCredentialsProvider` — the consumer that lets the default credentials chain recognise a `login_session = …` profile written by `aws login`. Enabling the feature does not introduce any CLI surface; it only lets the SDK consume credentials that the operator already obtained externally. The canonical spec page [`docs/specs/11-credentials-and-sso.md`](../../specs/11-credentials-and-sso.md) records the enable as a Decision. The plan that drove the enable lives at [`../plans/2026-05-19-credentials-login.md`](../plans/2026-05-19-credentials-login.md).

This file is kept for history; subsequent reviewers may want to see what was proposed and why the CLI-surface portion was rejected before reopening the topic. The credentials-consumption portion is not rejected — only the auth-driving CLI surface.
