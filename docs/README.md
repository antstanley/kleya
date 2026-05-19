# kleya docs

This directory holds the canonical design specifications for `kleya`. Each file is the authoritative reference for the area it describes — the source of truth for what is checked into `main`, not a wishlist.

End-user documentation lives in [`../README.md`](../README.md); contributor / agent documentation lives in [`../CONTRIBUTING.md`](../CONTRIBUTING.md). This directory is where you go when the question is "how does kleya actually work?" or "what shape does X take in the code?".

## Canonical specs

Numbered files in [`specs/`](specs/) are read in order; the JSON Schema sidecar mirrors the typed entities described in prose.

| Page | Topic |
|---|---|
| [specs/00-overview.md](specs/00-overview.md) | Purpose, goals, non-goals, system shape, scope summary |
| [specs/01-domain-model.md](specs/01-domain-model.md) | Newtypes, regexes, entity tables, lifecycle diagrams for instance / template / key |
| [specs/02-cli-surface.md](specs/02-cli-surface.md) | Commands, flags, environment variables, exit codes |
| [specs/03-configuration.md](specs/03-configuration.md) | Multi-format loader, precedence, canonical schema, validation, round-trip property |
| [specs/04-provider-port.md](specs/04-provider-port.md) | `CloudCompute` and `KeyStore` traits; idempotency contract; AWS adapter mapping |
| [specs/05-bootstrap-rendering.md](specs/05-bootstrap-rendering.md) | User-data template, ghostty terminfo, size budgets, override semantics |
| [specs/06-launch-and-connect.md](specs/06-launch-and-connect.md) | Launch orchestration, key lifecycle, connect flow, SSH probe, cloud-init wait, cancellation |
| [specs/07-error-model.md](specs/07-error-model.md) | `Error` enum, exit-code mapping, adapter boundary |
| [specs/08-testing.md](specs/08-testing.md) | Test tiers, Floci, snapshots, property tests, coverage floor |
| [specs/09-architecture-principles.md](specs/09-architecture-principles.md) | Crate layout, hexagonal layering, dependency graph, runtime / process model |
| [specs/10-development-guidelines.md](specs/10-development-guidelines.md) | Tiger-style rules, named limits, hooks, CI, release process |
| [specs/canonical-types.schema.json](specs/canonical-types.schema.json) | JSON Schema (Draft 2020-12) for every domain entity, config struct, and error variant |

Start with the overview if you're new; jump to the relevant numbered file for everything else. Every page closes with an `Assumptions / Decisions / Open questions` block.

## Historical drafts

The design docs that preceded this canonical spec set live under [`superpowers/specs/`](superpowers/specs/):

- [2026-05-16-kleya-bootstrap-design.md](superpowers/specs/2026-05-16-kleya-bootstrap-design.md) — the original monolithic design doc.
- [2026-05-18-review-fixes-design.md](superpowers/specs/2026-05-18-review-fixes-design.md) — the review-fix delta applied before v0.1.0-rc.2.

They are kept for context and for tracing decisions back to their original framing, but the canonical specs above supersede them.
