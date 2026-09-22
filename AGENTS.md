# Caster agent guide

## Session start

1. Read `IMPLEMENTATION_PLAN.md` and work on the earliest unchecked dependency
   that is relevant to the request.
2. Read `ARCHITECTURE.md`, then only the ADRs related to the subsystem being
   changed.
3. Inspect `git status` and preserve unrelated or user-owned changes.
4. Run the smallest relevant tests before editing and the full required checks
   before checking off a plan item.

## Invariants

- Never fall back from a selected window to screen capture.
- Application audio never falls back to system audio.
- Match windows and audio processes by stable identity, not friendly names.
- Keep latency bounded: queues may drop stale video but must not grow without
  limit.
- Stop and failure paths must restore audio and terminate every owned process.
- QML treats CLI output as data and never builds shell command strings.
- Logs must not include window titles unless explicit debug logging is enabled.

## Architecture rules

- New host/device support implements the traits in `omarchy-caster-core`.
- Use compile-time adapters and capability reporting; v1 has no runtime ABI.
- Put constants in the module that owns their meaning. Only application/API
  identifiers belong in the shared constants module.
- Prefer a narrowly named module over a generic `utils`, `common`, or `globals`
  module.
- Update `ARCHITECTURE.md` when the current design changes. Add an ADR only for
  a consequential, hard-to-reverse decision; supersede accepted ADRs rather
  than rewriting their history.

## Required checks

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
omarchy plugin validate .
```

The ignored live audio test may only run when no application-audio cast is
active. Hardware checklist items require explicit TV-side confirmation.
