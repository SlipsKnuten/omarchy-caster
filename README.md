# Caster

Low-latency Google Cast mirroring for [Omarchy](https://omarchy.org). Cast one
Hyprland application window or a whole screen to a Chromecast or Google TV from
a bar widget, without opening Chrome.

- Window capture that is surface-based: another window covering the selected
  app is never captured, and it never silently falls back to a screen crop.
- Application audio isolation: only the chosen app's sound goes to the TV and
  everything else stays local. System audio is available for screen casts.
- Native Cast Streaming through a small patched Open Screen sender, with
  H.264 video and Opus audio.
- Audio and video are timestamped on one capture clock so they stay in sync.
  See [ADR 0006](docs/adr/0006-shared-capture-clock-for-av-sync.md).

> **This plugin needs more than `omarchy plugin add`.** The plugin only clones
> the widget. The `caster` program and a patched Cast sender are installed
> separately, as described under [Install](#install). Until they are present the
> widget shows a "Setup required" panel instead of casting.

## Status

Tested on one machine: Omarchy with Hyprland on AMD integrated graphics, casting
to a 4K Google TV Stick. Window capture with application audio is the verified
path. Whole-screen capture still goes through `wf-recorder`, which does not
carry capture timestamps yet, so screen casts may show a larger A/V offset.
Hardware coverage and the acceptance checklist are in
[docs/hardware-verification.md](docs/hardware-verification.md); items there
that are unchecked have not been verified. Other receivers are untested.

## Requirements

Runtime:

| Need | Arch package |
|---|---|
| Hyprland session on Omarchy | `hyprland` |
| PipeWire's PulseAudio server | `pipewire-pulse` |
| Screen capture compatibility path | `wf-recorder` |
| FFmpeg libraries | `ffmpeg` |
| systemd user manager | `systemd` |
| Patched Open Screen sender | built below, installed as `caster-openscreen` |

Build:

| Need | Arch package |
|---|---|
| Rust toolchain | `rust` |
| bindgen (for the FFmpeg bindings) | `clang` |
| Wayland and Pulse headers | `wayland`, `libpulse` |
| pkg-config | `pkgconf` |
| Open Screen build (sender only) | `git`, `python`, `opus`, `libvpx`, `sdl2` and [depot_tools](https://chromium.googlesource.com/chromium/tools/depot_tools) |

The sender build pulls a large Chromium dependency tree and takes a while. It is
the only slow step.

## Install

All steps run as your own user and nothing needs root. The programs are
installed into `~/.local/bin`, the widget into `~/.config/omarchy/plugins`, and
Caster keeps its own data in XDG directories. The build directories (this
checkout, `depot_tools`, the Open Screen tree) can be deleted afterwards.

### 1. Build and install `caster`

```sh
git clone https://github.com/SlipsKnuten/omarchy-caster.git
cd omarchy-caster
cargo build --release --locked
install -Dm755 target/release/caster ~/.local/bin/caster
```

`~/.local/bin` must be on your `PATH`.

### 2. Build and install the patched Open Screen sender

The sender is [Chromium Open Screen](https://chromium.googlesource.com/openscreen)
at the commit pinned in [native/openscreen-revision.json](native/openscreen-revision.json),
plus the small live-input patch in
[native/openscreen-live-input.patch](native/openscreen-live-input.patch). The
patch applies cleanly to that commit.

```sh
git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git ~/depot_tools
export PATH="$HOME/depot_tools:$PATH"

mkdir ~/openscreen-src && cd ~/openscreen-src
fetch openscreen
cd openscreen
git checkout 648ab4c0b05f9f464e36e4c89ef0559cb61b0b81
gclient sync

git apply /path/to/omarchy-caster/native/openscreen-live-input.patch

buildtools/linux64/gn gen out/Release --args='is_debug=false have_ffmpeg=true have_libopus=true have_libvpx=true have_libsdl2=true use_sysroot=false is_component_build=false'
third_party/ninja/ninja -C out/Release cast_sender

install -Dm755 out/Release/cast_sender ~/.local/bin/caster-openscreen
```

The fetch and sync commands follow upstream's documented depot_tools flow. The
build arguments are the ones used for the local build this plugin was developed
with. If a system FFmpeg upgrade breaks the sender, rebuild it.

Developers can skip the install and point at a build directly with
`CASTER_SENDER=/path/to/cast_sender`, or `development.sender` in
`$XDG_CONFIG_HOME/caster/config.json`.

### 3. Add the plugin

```sh
omarchy plugin add https://github.com/SlipsKnuten/omarchy-caster.git --enable
```

Choose a bar section when prompted. The widget is `slipsknuten.caster`.

### 4. Check it

```sh
caster doctor
```

Every backend should report `"available": true`. The widget runs the same check:
if something is missing it lists exactly what, with a button to check again
after you have fixed it.

## Use

Click the bar icon, choose a receiver, then an application or a screen, then
choose whether to send audio and press **Cast now**. Right-click the icon
to rescan for receivers.

From a terminal:

```sh
caster devices
caster sources
caster start --device RECEIVER_ID --source SOURCE_ID \
  --audio off --quality auto --bitrate auto
caster status
caster stop
```

Add `--json` for the versioned machine-readable envelope used by the widget:
`{"api":1,"ok":true,"result":...}` or a structured error.

## Update

```sh
omarchy plugin update slipsknuten.caster
```

This updates the widget only. After a plugin update, rebuild and reinstall
`caster` (step 1) so the program and the widget stay on the same version.

## Remove

Stop any running cast first:

```sh
caster stop
omarchy plugin remove slipsknuten.caster
rm -f ~/.local/bin/caster ~/.local/bin/caster-openscreen
```

Caster's own data is the only thing left behind. It is safe to delete:

```sh
rm -rf ~/.config/caster ~/.local/state/caster "$XDG_RUNTIME_DIR/caster"
```

Caster does not edit shell, Hyprland or audio configuration permanently. While
casting an application's audio it moves only that application's stream to a
private temporary sink, and it restores the stream when the cast stops, fails,
or the worker is recovered.

## Configuration and privacy

Caster uses private `caster` directories under the standard XDG config, runtime
and state roots. It remembers only the receiver ID, source kind, audio mode,
quality and bitrate. It does not persist window titles or identities, never
starts casting by itself, and never writes media to disk.

Application audio is matched by process identity and ancestry and never falls
back to the system mix. Logs can contain window titles when debug logging is
enabled, so redact them before sharing.

The plugin runs unsandboxed inside the Omarchy shell, like every shell plugin.
`caster` itself runs your session's cast worker as a transient systemd user
service and invokes `hyprctl`, `pactl` and `wf-recorder`; it uses no `sudo`
and no `pkexec`.

## Architecture

The Cargo workspace contains:

- `omarchy-caster-core`: domain types, API envelopes, adapter traits, quality
  policy and declarative receiver profiles.
- `omarchy-caster-capture-hyprland`: Hyprland surface capture, scaling and video
  NUT output.
- `omarchy-caster-audio-pulse`: Pulse-compatible capture and audio NUT output.
- `omarchy-caster-cli` (`caster`): discovery, routing, session supervision,
  configuration and the CLI boundary.

See [ARCHITECTURE.md](ARCHITECTURE.md), [AGENTS.md](AGENTS.md), the decisions
under [docs/adr](docs/adr), and [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

## Development

```sh
cargo build --release --locked
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
omarchy plugin validate .
qmllint -I /usr/share/omarchy/shell \
  plugin/BarWidget.qml plugin/Service.qml plugin/CastChoiceRow.qml
```

The live audio-routing test is ignored by default because it creates local test
streams and changes their routing. Run it only when no cast is active:

```sh
cargo test live_routing -- --ignored --test-threads=1
```

New OS, compositor, audio, discovery, transport or supervisor support must
implement the compile-time traits in `omarchy-caster-core`, report
capabilities, fail closed, include fixtures and tests, and attach hardware
evidence. Receiver quirks belong in the declarative profile data keyed by
stable model or negotiated capabilities, not user-editable friendly names.
There is intentionally no runtime code-plugin ABI.

## License

Original code is GPL-3.0-only. Bundled Wayland protocol XML retains its embedded
license notices. Open Screen, FFmpeg, PulseAudio and their dependencies retain
their respective upstream licenses.
