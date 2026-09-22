# ADR 0002: Compile-time adapters before a runtime plugin ABI

- Status: Accepted
- Date: 2026-09-22

## Context

Caster should be approachable to contributors adding receiver devices or host
operating systems, but the core contracts are new and handle privacy-sensitive
capture and audio.

## Decision

Use documented Rust traits, a static registry, feature flags, typed capability
reporting, and declarative receiver profiles. Do not load third-party shared
libraries or executable backends through a public runtime ABI in v1.

## Consequences

Adapters are reviewed and built with Caster. Contributors get explicit seams
without an ABI compatibility or arbitrary-code-loading promise. A subprocess
protocol may be proposed later through a new ADR if independent distribution
becomes more valuable than this safety boundary.
