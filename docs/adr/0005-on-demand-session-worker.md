# ADR 0005: On-demand supervised session worker

- Status: Accepted
- Date: 2026-09-22

## Context

Casting is intermittent. An always-on daemon simplifies event delivery but adds
a permanent service and another local protocol before it is needed.

## Decision

Run a supervised transient worker only while a cast is active. Expose the
supervisor through an adapter so future hosts can use launchd or Windows service
management. Publish atomic status and let the Omarchy panel watch it.

## Consequences

Idle Caster has no daemon. Stop and recovery must be idempotent, and the status
file/API must make unexpected worker loss explicit.
