# Contributing to Caster

Start with `AGENTS.md` and `ARCHITECTURE.md`. Keep changes scoped to one adapter
or contract and include tests. A compatibility contribution must provide:

1. Capability detection that fails closed.
2. Fixtures for external protocol or command data.
3. Unit/integration coverage for normal and cleanup paths.
4. Hardware evidence using `docs/hardware-verification.md`.
5. Documentation updates without claiming untested devices.

Run the required checks in `AGENTS.md`. Do not commit window titles, local IPs,
receiver IDs, logs, build artifacts, or private configuration.

Use an ADR for decisions that change a public contract, privacy boundary,
process architecture, or long-term dependency. Ordinary implementation details
belong in code and tests.
