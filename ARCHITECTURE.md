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

The current migration still uses the tested native helper, Pulse-compatible
commands, FFmpeg, and a patched Open Screen executable behind those boundaries.
The production-readiness work replaces host-side helpers with Rust adapters
without changing the media/transport contract all at once.

## Crate and adapter boundaries

- `omarchy-caster-core` owns domain types, public DTOs, quality policy, API
  versioning, and adapter traits. It has no Omarchy or process-management code.
- The CLI/controller resolves compile-time adapters, validates user intent,
  stores requests atomically, and supervises one session worker.
- Capture owns source enumeration, stable source identity, frame acquisition,
  scaling, and source-lifetime validation.
- Audio owns application matching, private routing, capture, and restoration.
- Transport owns receiver discovery, negotiation, Cast-specific profiles, and
  sender events.
- Supervision owns systemd interaction and process-tree cleanup.
- QML owns presentation only and consumes the versioned JSON API.

Adapters publish capabilities. Unsupported behavior is disabled explicitly;
one adapter may never silently substitute a broader privacy scope.

## State and configuration

Configuration, runtime state, and logs use separate XDG roots. Runtime JSON is
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
