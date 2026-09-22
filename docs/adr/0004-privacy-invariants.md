# ADR 0004: Privacy scope never widens implicitly

- Status: Accepted
- Date: 2026-09-22

## Context

Screen capture and system audio are broader than a selected application. A
fallback can expose unrelated windows, notifications, or conversations.

## Decision

Window capture may only use a surface backend and stops if stable identity can
no longer be verified. Application audio may only use process identity and
ancestry and sends silence when it cannot match. Broader screen/system capture
requires an explicit user selection.

## Consequences

Compatibility failures are visible rather than silently helpful. Device
profiles, UI code, and future adapters may not override these invariants.
