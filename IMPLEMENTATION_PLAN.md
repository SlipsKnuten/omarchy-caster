# Caster production-readiness plan

This is the implementation ledger for the local release candidate. Check an
item only after its acceptance condition has been verified. Deployment, AUR
packaging, GitHub publication, the public Open Screen fork, and Omarchy store
submission are deliberately deferred.

## 1. Repository and project contract

- [ ] Rebrand product surfaces to **Caster**: binary `caster`, plugin ID
  `slipsknuten.caster`, repository name `omarchy-caster`, and prefixed Rust
  package names.
- [ ] Convert the source tree into a documented Rust workspace with narrow
  crates for core/domain logic, CLI, media, Hyprland capture, Pulse audio,
  Google Cast transport, and systemd supervision.
- [x] Initialize clean local Git history without publishing or pushing it.
- [ ] Remove obsolete Python caches and, after parity verification, the C
  capture helper and Makefile.
- [ ] Keep the current development plugin and FluxCast launcher available as
  rollback paths until final hardware acceptance.
- [x] Add `AGENTS.md`, a living `ARCHITECTURE.md`, numbered immutable ADRs,
  `CONTRIBUTING.md`, `SECURITY.md`, a changelog, and a hardware evidence
  template.
- [ ] Add formatting, lint, test, dependency/license, and plugin-validation CI
  definitions without publishing artifacts.

## 2. Typed architecture and extension boundaries

- [ ] Replace stringly session/source/status values and untyped JSON parsing
  with domain enums and typed DTOs.
- [x] Define compile-time adapter traits for receiver discovery, capture,
  audio, transport, and session supervision.
- [x] Add a static feature-gated backend registry and typed capability report;
  do not add a runtime plugin ABI in v1.
- [x] Add declarative Google Cast receiver profiles keyed by stable model and
  capability data, never friendly names.
- [x] Keep only product-wide constants in a shared constants module; colocate
  timeouts, protocol values, and defaults with their owners.
- [ ] Use narrowly named XDG, process, atomic-storage, and serialization
  modules rather than generic `globals` or `utils` folders.
- [x] Add fake adapters and fixture-driven contract tests so new OS/device
  implementations can be tested without hardware.

## 3. Full Rust host media pipeline

- [x] Replace the C window helper with Rust Hyprland toplevel-export capture.
- [ ] Replace `wf-recorder` with Rust wlroots screencopy output capture.
- [x] Generate Wayland bindings from bundled protocol XML at build time.
- [ ] Reuse shared-memory buffers, handle source resize/closure, pace from a
  monotonic clock, and keep only the newest pending video frame.
- [x] Scale and letterbox without stretching into a stable I420 canvas.
- [ ] Replace FFmpeg subprocesses with Rust FFmpeg bindings for conversion and
  NUT framing.
- [ ] Replace repeated `pactl` polling with a Rust Pulse-compatible adapter for
  stream discovery, subscription, private-sink routing, capture, and recovery.
- [ ] Start audio and video from one monotonic epoch and preserve timestamps
  through the Open Screen boundary.
- [ ] Prove cleanup restores audio and leaves no worker, private sink, or Cast
  session after normal stop, crash, source loss, or receiver failure.

## 4. Google Cast transport and quality negotiation

- [x] Turn the Open Screen modifications into a clean local fork commit series
  pinned to an upstream revision; public fork publication remains deferred.
- [x] Replace log scraping with versioned JSON-lines events on a dedicated file
  descriptor.
- [ ] Emit negotiation, receiver display constraints, resolved stream quality,
  health metrics, scheduled playout, and structured fatal errors.
- [ ] Start capture after negotiation so Auto uses receiver display and capture
  recommendations rather than a model-name guess.
- [ ] Support receiver-aware Auto, 720p30, and 1080p30 while preserving source
  proportions; cap current mirroring at 1920x1080/30 fps.
- [x] Support Auto, 4, 6, and 8 Mbps bitrate choices, clamped to receiver
  recommendations.
- [ ] Add bounded reconnect/backoff behavior for rapid receiver reconnects.

## 5. Lifecycle, configuration, and CLI

- [x] Keep an on-demand transient worker and place systemd operations behind a
  typed Rust supervisor adapter.
- [x] Use typed session states: idle, discovering, connecting, negotiating,
  streaming, stopping, and error.
- [x] Make stop idempotent and ensure an already-ended session returns to idle
  instead of retaining stale error state.
- [ ] Add versioned configuration and state under Caster XDG directories.
- [x] Remember only the last receiver, source kind, audio preference, and
  quality; never persist a window title/identity or auto-start a cast.
- [x] Remove production dependence on machine-specific absolute binary paths;
  retain explicit development overrides only.
- [x] Provide human output by default and a versioned `--json` envelope for:
  `devices`, `sources`, `start`, `status`, `stop`, and `doctor`.
- [ ] Keep worker/recovery commands internal and document public exit/error
  behavior.
- [ ] Redact sensitive window titles from logs by default while keeping them in
  the live source picker where the user needs them.

## 6. Omarchy GUI redesign

- [x] Move the manifest to repository root and validate the repository directly
  with `omarchy plugin validate`.
- [x] Replace the development `PopupCard` with Omarchy's native `Panel` and
  first-party panel primitives.
- [x] Split QML into a panel, pure state/model logic, and narrowly scoped shared
  components.
- [x] Add a clear idle flow: receiver rows, Applications/Screens selector,
  contextual audio choice, one Cast action, and folded Advanced settings.
- [ ] Add an active view showing receiver, source, resolved quality, audio,
  health, connection stages, and a prominent Stop action.
- [ ] Add useful loading, empty, backend-missing, source-lost, receiver-failed,
  and recovery states with Retry/Refresh/Diagnostics actions.
- [ ] Support mouse and keyboard navigation, long labels, narrow panels, and
  horizontal/vertical bars using Omarchy theme primitives.
- [x] Watch the atomic status file instead of spawning `caster status` every
  second; discover only on panel open or explicit refresh.
- [x] Serialize UI actions so rapid clicks cannot race start and stop.

## 7. Automated verification

- [x] `cargo fmt --all --check` passes.
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  passes.
- [x] All unit and integration tests pass with the lockfile.
- [ ] Synthetic capture output decodes as valid I420/NUT at every supported
  preset and survives resize/format/orientation changes.
- [ ] Audio tests cover ancestry, new streams after start, PID reuse, isolation,
  restoration, and forced failure.
- [x] API fixtures lock JSON compatibility and every session state transition.
- [x] QML lint/model tests pass and `omarchy plugin validate` accepts the root.
- [ ] Dependency/license checks account for bundled protocols, FFmpeg/Pulse
  bindings, and the Open Screen fork.

## 8. Hardware acceptance

- [ ] Window casting works on Family room TV and Master Bedroom TV.
- [ ] Screen casting works on both receivers.
- [ ] Window occlusion, resize, minimize, workspace movement, and closure never
  leak another window or silently fall back to screen capture.
- [ ] Application audio isolates the selected app while another app remains
  local; optional screen audio behaves as labeled.
- [ ] Flash/beep A/V offset is no greater than 100 ms.
- [ ] Visible end-to-end latency is at or below one second.
- [ ] Receiver-aware Auto and manual 720p/1080p modes are clear, proportional,
  and stable.
- [ ] Ten repeated start/stop/reconnect cycles leave no stale process, audio
  route, sink, or receiver session.
- [ ] A 60-minute 1080p30 session has no unbounded memory growth and uses no
  more CPU or memory than the current working prototype.
- [ ] All everyday actions can be completed from the Caster panel without
  opening Chrome or a terminal.

## Deferred release work

- [ ] Publish the main GitHub repository and maintained Open Screen fork.
- [ ] Produce signed/reproducible release binaries and an AUR package.
- [ ] Submit the plugin through the current Omarchy review process.
- [ ] Add other Wayland compositors, macOS, Windows, AirPlay, DLNA, hardware
  encoding, 4K, or 60 fps only through the documented adapter/capability model.
