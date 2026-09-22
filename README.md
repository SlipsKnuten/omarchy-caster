# Native Cast — development working name

Rust-controlled native Chromecast mirroring for Omarchy/Hyprland. This is a
local prototype, not a published release. The final project name is pending:
several existing projects already use OmaCast.

## Components

- `src/`: Rust CLI, receiver discovery, configuration, session supervision,
  source identity, audio routing, and native transport adapter.
- `native/`: small C Hyprland surface-capture helper. Captures only the chosen
  window, scales onto a 1080p canvas without stretching, and emits raw NUT video.
- `plugin/`: QML Omarchy bar widget. Uses the same versioned JSON CLI as scripts.

No Python runs in this implementation. Open Screen C++ handles H.264 encoding
and native Cast Streaming; FFmpeg handles audio capture. Omarchy Quickshell owns
the GUI. Rust is used for control and safety, not as a claim of lower latency
by itself. End-to-end latency still requires a visual TV comparison.

## Build and development setup

```sh
make -C native
cargo build --release --locked
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

Dependencies: Rust, C compiler, make, wayland-scanner, Wayland and FFmpeg
development libraries, wf-recorder, ffmpeg, pactl, Hyprland, user systemd,
and the patched Open Screen sender from our preceding hardware-tested prototype.
Window capture requires Hyprland's toplevel-export protocol.

Configure `sender` and `window_capture` as absolute native executable paths in
`~/.config/omacast-dev/config.json`. These machine-specific paths are deliberately
outside the repository. Current development launch integration points to the
local release binary; do not delete the checkout while using the widget.

CLI: `devices`, `sources`, `start --device ID --source ID --audio off|application|system`,
`status`, `stop`, `doctor`. Refresh devices and sources before starting. Each
response is JSON: `{ "api": 1, "ok": true, "result": ... }`, or an error.
Friendly names are display data, not shell commands or device identities.

## Privacy and lifecycle

Window capture uses the window surface, never a desktop region. The user's
explicit source choice in the GUI authorizes capture. This Hyprland-specific
backend does not display a second portal prompt. Capture stops when source
identity changes; it never switches to desktop capture.

Application audio is matched only by process identity/ancestry. Matching
Pulse-compatible PipeWire streams move into a private temporary sink, so they
play on the TV rather than local speakers. Other applications keep their output.
Browser audio is application-wide, not tab-isolated. Missing/ambiguous identity
means silence, never a fallback to whole-system audio. Native PipeWire streams
not exposed through the Pulse interface are not supported by this audio adapter.
There can be a brief local-audio interval before newly created streams are routed.

The GUI offers whole-system audio only for screen mode, and only by explicit
selection. Screen mode does not promise TV-only local muting.

The user service supervises children and restores audio on normal stop/error.
ExecStopPost retries audio recovery if the worker is killed. Recovery only moves
streams still on our unique sink and checks process start time before acting.
No persistent PipeWire config, firewall rule, keybinding, or boot autostart is added.

Diagnostics: `journalctl --user -u omacast-dev.service` and
`~/.local/state/omacast-dev/session.log`. Logs/state stay private and can contain
window titles/device names; redact before sharing. Media is not recorded.

## Extension boundaries

Capture owns compositor/window identity and source lifetime. Audio owns matching,
temporary routing and recovery. Discovery owns receiver identity and addresses.
Transport owns Open Screen-specific command/configuration. Session owns their
lifetime and failure handling; neither QML nor a device profile may override
privacy invariants. Configuration holds user policy; protocol constants stay in
their owning modules. Shared types are limited to source, receiver, session and
quality data rather than a generic globals/utils directory.

Contributors should add an adapter in the relevant subsystem, capability checks,
tests and hardware evidence. Do not identify device behavior solely from its
friendly name. Runtime downloadable code plugins, other desktops/protocols,
hardware encoding and automatic TV resolution selection are not implemented.

## Release gates

Before publication: validate GUI, window privacy under occlusion/resize/workspace
changes, application audio with two simultaneous apps, restoration after failure,
subsecond visible latency, and extended stability. Add reproducible Open Screen
build packaging, dependency/license inventory, clean-environment CI and Arch
packages. Preserve the existing FluxCast Live launcher as rollback until verified.

### Local verification (2026-09-22)

- Rust unit tests cover source/process identity, capture command separation,
  audio-mode privacy constraints, safe transport arguments, and rejection of
  invalid/NaN stream statistics.
- A live silent-audio test verifies that one process is rerouted, a second stays
  untouched, and recovery from saved state restores the original sink and unloads
  the private sink. Run explicitly with `cargo test live_routing -- --ignored
  --test-threads=1`; never run it during an application-audio cast.
- The window helper's actual frames decode as I420/YUV420P at 1920x1080. The first
  build had a wrong raw-video codec tag; this was fixed after full decode testing.
- Clock-window native streaming reports finite ~410 ms scheduled playout and
  ~20 fps for a clock that redraws at 20 Hz. This excludes upstream capture and
  is NOT a visual TV-side latency or quality confirmation.
- The application-audio capture adapter is Pulse-compatible PipeWire only.
  Native PipeWire-only clients are not claimed to work. TV audio and crash-path
  service cleanup still require end-to-end validation.
- User confirmed isolated-window casting on **Master Bedroom TV**, approximately
  one-second visible delay, and no covering-window leakage. Earlier attempts
  targeted Family room TV; do not conflate those receivers' test evidence.
- Audible two-application test started on Master Bedroom TV: selected ffplay
  window has a low 440 Hz beep every two seconds; a separate ffplay process has
  an 880 Hz beep every second. Routing inspection confirms the first on the
  private sink and the second on the original laptop sink. Actual TV listening
  confirmation and stop/restoration validation are pending.

## License and attribution

Original code: GPL-3.0-only. Bundled Wayland XML protocols retain their embedded
BSD-style notices. The external Open Screen sender and its dependencies retain
their own licenses. The discovery design was informed by Aphrodine-wq/omacast
(MIT); GUI integration follows Omarchy's documented component interfaces.
