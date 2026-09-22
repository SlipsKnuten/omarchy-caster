# ADR 0003: Maintain Open Screen as the Cast engine boundary

- Status: Accepted
- Date: 2026-09-22

## Context

The tested sub-second stream depends on Chromium Open Screen's Cast Streaming
sender plus small live-input changes. A new Rust sender would require a large,
high-risk protocol implementation before Caster's host application is mature.

## Decision

Maintain a minimal, reviewable Open Screen fork pinned to an upstream commit.
Communicate with it through media pipes and a versioned JSON-lines event file
descriptor. Keep negotiation, encoding, congestion control, and receiver
session behavior inside that boundary.

## Consequences

Caster can harden its Rust host pipeline without claiming that C++ is removed.
Fork publication and reproducible release binaries are release-phase work; the
local release candidate preserves an exact patch and upstream revision.
