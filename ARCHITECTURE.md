# Caster architecture

Caster is a native Google Cast mirroring application for Omarchy. The first
supported host is Hyprland on Omarchy; portability is provided through typed
adapter boundaries rather than unverified compatibility claims.

## Data flow

```text
Omarchy Panel / CLI
        |
        v
versioned command API ----> XDG configuration and atomic status
        |
        v
on-demand session worker
   |          |             |
   v          v             v
capture     audio      Google Cast transport
   |          |             |
   +---- timestamped NUT ---+
                          Open Screen sender --> receiver
```

Window capture, video conversion/framing, and Pulse-compatible audio capture now
run in Rust. Whole-output capture temporarily uses `wf-recorder`; application
audio routing temporarily uses focused `pactl` operations. A patched Open Screen
executable remains the stable C++ transport boundary. The earlier C window
helper stays in the tree only as a rollback artifact until hardware parity is
signed off.

## A/V timing

Audio and video are captured by separate processes, so both stamp every packet
with its capture time on `CLOCK_MONOTONIC` (microseconds). Video stamps when the
frame is acquired. Audio derives packet times from its sample count, corrected
toward the observed time (`now - pa_simple_get_latency - duration`): small
errors are slewed to follow device clock drift, larger ones re-anchor after a
stall. The sender back-dates each packet by its age, so the receiver sees the
true capture offset rather than arrival order. The audio capture stream is
requested with 20 ms fragments and at most 200 ms of server-side buffering;
PulseAudio's default is seconds, which turned any startup stall into a
permanent audio delay. Timestamps more than 30 s old fall back to arrival
time with a warning, which covers older helpers. See ADR 0006. The
`wf-recorder` screen path does not yet carry capture time and still uses
arrival-time stamping.

## Crate and adapter boundaries

- `omarchy-caster-core` owns domain types, public DTOs, quality policy, API
  versioning, and adapter traits. It has no Omarchy or process-management code.
- The CLI/controller resolves compile-time adapters, validates user intent,
  stores requests atomically, and supervises one session worker.
- Capture owns source enumeration, stable source identity, frame acquisition,
  scaling, and source-lifetime validation.
- Audio owns application matching, private routing, direct Pulse capture, NUT
  framing, and restoration.
- Transport owns receiver discovery, negotiation, Cast-specific profiles, and
  sender events.
- Supervision owns systemd interaction and process-tree cleanup.
- QML owns presentation only and consumes the versioned JSON API.

Adapters publish capabilities. Unsupported behavior is disabled explicitly;
one adapter may never silently substitute a broader privacy scope.

## State and configuration

Versioned configuration, runtime state, and logs use separate `caster` XDG
roots. Runtime JSON is
written atomically with private permissions. The public session state machine
is `idle -> connecting -> negotiating -> streaming -> stopping -> idle`, with
`error` as a cleaned-up terminal state. Stop is idempotent.

Only receiver and non-sensitive preferences persist. Window identities and
titles are live data and are never saved as preferences. Media is never written
to disk.

## Extension model

New support is submitted as a compile-time adapter plus capability checks,
fixtures, automated tests, and hardware evidence. Google Cast device quirks use
declarative profiles keyed by stable model/capability data. Friendly names are
display data only.

The project does not promise a stable dynamic plugin ABI. This keeps unsafe
media and OS integrations reviewable while the core contract matures.

## Security and privacy

Window capture is surface-based. Closing or changing the selected source stops
the session. Application audio uses process identity and ancestry and never
falls back to system audio. All external programs receive structured argument
arrays, never interpolated shell text. Cleanup state records enough identity to
restore only streams Caster moved.

See `docs/adr/` for the rationale behind binding decisions and
`docs/hardware-verification.md` for verified compatibility claims.

The transport fork is pinned by `native/openscreen-revision.json`. Its Caster
change is one commit on top of the recorded Chromium Open Screen revision, and
the same diff is retained as `native/openscreen-live-input.patch` for review and
reproduction until the public fork is published.
