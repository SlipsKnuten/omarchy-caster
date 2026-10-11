# Caster

Caster is an open-source, low-latency Google Cast mirroring app for Omarchy.
It can share one Hyprland application surface or a complete output without
opening Chrome. The project is a local release candidate and is not published
or packaged yet.

## What works today

- Rust mDNS discovery for Google Cast receivers.
- Privacy-preserving Hyprland window capture using toplevel-export; another
  window covering the selected app is never captured.
- Whole-output capture through the temporary `wf-recorder` compatibility path.
- Rust FFmpeg bindings for window scaling, stable I420 letterboxing, video NUT
  framing, and Pulse-compatible audio NUT framing.
- Application-audio isolation through a private Pulse/PipeWire sink, plus
  optional system audio for whole-screen sharing.
- Patched Open Screen C++ transport with the native Cast Streaming receiver,
  subsecond hardware-tested latency, and synchronized audio/video.
- An on-demand systemd user worker and a native Omarchy panel.

The installed development prototype and FluxCast remain rollback paths. The new
Rust window pipeline has automated media verification but still needs an
unlocked-session and TV-side acceptance pass before replacing that prototype.

## Architecture

The Cargo workspace currently contains:

- `omarchy-caster-core`: domain types, API envelopes, adapter traits, quality
  policy, and declarative receiver profiles.
- `omarchy-caster-capture-hyprland`: Hyprland surface capture, scaling, and
  video NUT output.
- `omarchy-caster-audio-pulse`: direct Pulse-compatible capture and audio NUT
  output.
- `omarchy-caster-cli` (`caster`): discovery, routing, session supervision,
  configuration, and the CLI boundary.

See [ARCHITECTURE.md](ARCHITECTURE.md), [AGENTS.md](AGENTS.md), and the accepted
decisions under [docs/adr](docs/adr). Progress and acceptance criteria live in
[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

## Build and verify

Required development libraries include Wayland, FFmpeg, and PulseAudio; runtime
requirements include Hyprland, PipeWire-Pulse, systemd user services,
`wf-recorder` for screen compatibility, and the patched Open Screen sender.

```sh
cargo build --release --locked
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
omarchy plugin validate .
qmllint -I /usr/share/omarchy/shell \
  plugin/BarWidget.qml plugin/Service.qml plugin/CastChoiceRow.qml
```

The live audio-routing test is intentionally ignored because it creates local
test streams and changes their routing. Run it only when no cast is active:

```sh
cargo test live_routing -- --ignored --test-threads=1
```

## CLI

Human-readable output is the default:

```sh
caster devices
caster sources
caster start --device RECEIVER_ID --source SOURCE_ID \
  --audio off --quality auto --bitrate auto
caster status
caster stop
caster doctor
```

Automation and QML use `caster --json COMMAND`. That output is a versioned
envelope: `{"api":1,"ok":true,"result":...}` or a structured error. Internal
worker, recovery, and media commands are hidden from help and are not public API.

## Configuration and privacy

Caster uses private `caster` directories under the standard XDG config,
runtime, and state roots. It remembers only receiver ID, source kind, audio,
quality, and bitrate preferences. It does not persist a window title or window
identity and never starts casting automatically.

Production finds the transport as `caster-openscreen` on `PATH`. Developers may
set `CASTER_SENDER` or `development.sender` in
`$XDG_CONFIG_HOME/caster/config.json`. During migration, the old
`omacast-dev/config.json` sender path is read as a compatibility fallback.

Window capture is surface-based and never falls back to a desktop crop.
Application audio matches process identity and ancestry and never falls back to
the system mix. Runtime request/status data is private and media is never saved
to disk. Logs should redact window titles before sharing.

## Contributing compatibility

New OS, compositor, audio, discovery, transport, or supervisor support must
implement the compile-time traits in `omarchy-caster-core`, report capabilities,
fail closed, include fixtures/tests, and attach hardware evidence. Receiver
quirks belong in the declarative profile data and are keyed by stable model or
negotiated capabilities—not user-editable friendly names. V1 intentionally has
no runtime code-plugin ABI.

## Current hardware evidence

The predecessor pipeline was confirmed on Master Bedroom TV with isolated
window capture, no covering-window leakage, synchronized audio/video, and about
one second visible delay. Those results establish the performance target; they
do not yet check off the replacement Rust pipeline. See
[docs/hardware-verification.md](docs/hardware-verification.md) for the full
acceptance matrix.

## License

Original code is GPL-3.0-only. Bundled Wayland protocol XML retains its embedded
license notices. Open Screen, FFmpeg, PulseAudio, and their dependencies retain
their respective upstream licenses.
