# Performance and power

Measured notes from development, so users know what to expect. They come from
one machine and are not a benchmark: an Omarchy laptop with AMD integrated
graphics, a 1920x1200 panel at scale 1.5, Hyprland 0.56.2, casting a browser
window to a Chromecast (Master Bedroom) and a 4K Google TV Stick. Treat the
numbers as the shape of the problem, not as guarantees.

## Summary

- Casting is **CPU-heavy** and therefore draws noticeably more power than idle.
  Prefer AC power, and expect shorter battery life while a cast runs.
- **Window capture is limited by the compositor.** When the casted window is on
  the visible, focused workspace, the delivered frame rate roughly halves
  compared with the same window on a hidden workspace.
- Nothing in Caster's own conversion path is the bottleneck, and no frames are
  dropped by Caster. The cost is in Hyprland's per-frame window export.

## Where the CPU goes

CPU percentages are `top` per-core figures (100% is one full core) sampled
every ~2 s during a window cast with application audio at 1080p30.

| Process | Idle (no cast) | Casting |
|---|---|---|
| Hyprland (compositor) | ~47-51% | **~90-97%** |
| Open Screen sender (x264 software encode, Cast) | n/a | **~95-170%** |
| `caster capture-window` | n/a | ~11-18% |
| `caster capture-audio` | n/a | under 1% |

Two things dominate: the compositor has to copy every captured frame out of the
GPU, and the sender encodes H.264 in software (`libx264`). Together they keep
more than two cores busy for the duration of the cast. Power draw has **not**
been measured in watts yet; the CPU figures above are the evidence.

## Focused versus hidden window

These numbers came from a development build of the capture helper that logs one
`capture-stats` line every 2 s. That telemetry is deliberately **not** in
`main`, so released builds do not write it to users' log files; it lives on the
`dev` branch. Across switching the casted window between a hidden workspace and
the focused one:

| Casted window | Compositor wait per export | Frames captured | Caster convert+write | Dropped |
|---|---|---|---|---|
| On a hidden workspace | ~42 ms | ~23-24 fps | ~9-10 ms | 0 |
| Visible and focused | ~85-92 ms | ~10-12 fps | ~10 ms | 0 |

"Compositor wait" is the time from asking Hyprland for a frame to receiving it.
It roughly doubles when the window is on screen, which halves the frame rate.
Caster's own scaling and writing is flat at about 10 ms in both states.

Practical consequence: if the picture on the TV looks choppy, move the casted
window to another workspace and keep working elsewhere. Casting a whole screen
goes through `wf-recorder` (screencopy) instead and held ~28 fps in the same
setup, but it shares everything on that output and has a larger A/V offset (see
[ADR 0006](adr/0006-shared-capture-clock-for-av-sync.md)).

## What would help (not done)

None of these has been tried, so none is a promise:

- **Capture through `ext-image-copy-capture-v1`** with the foreign-toplevel
  capture source. Hyprland 0.56 implements it, and it supports damage tracking
  and GPU buffers, which could cut the per-frame readback. Whether it is faster
  on Hyprland is unmeasured. A time-boxed spike that prints the same
  compositor-wait numbers would answer that before any backend is built.
- **Hardware H.264 encoding** (for example VAAPI) instead of `libx264`, to cut
  the sender's CPU and power use. The Open Screen sender currently only wires
  up software encoding.
- **Capping the export rate** (for example 15 fps) to keep the compositor and
  the rest of the desktop responsive, at the cost of lower frame rate on the TV.

## Reproduce

Build the `dev` branch and start a window cast, then read the telemetry:

```sh
git checkout dev
cargo build --release --locked
grep capture-stats ~/.local/state/caster/session.log | tail -20
```

Each line reports captured and written frames per second, frames replaced before
they were written, and mean/max compositor wait and convert+write time. Use the
`main` branch for normal use.
