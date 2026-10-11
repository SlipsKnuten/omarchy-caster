# ADR 0006: Stamp audio and video on one capture clock

- Status: Accepted
- Date: 2026-10-11

## Context

Audio and video are captured by separate helper processes and joined only in
the Open Screen sender. The sender originally ignored the NUT timestamps of
live inputs: video was stamped with its arrival time and audio with a
free-running sample counter anchored at the first packet. Neither tied a packet
to when it was captured, so the audible offset depended on startup order and
pipeline latency and was never corrected afterwards.

Investigation found two further causes of sustained audio delay:

- `pa_simple_new` was called without buffer attributes. PulseAudio's default
  record buffering is seconds, so audio arrived in large, old chunks.
- Once audio looked older than the sender's accepted window, the sender used
  the legacy free-running pacing, which consumes at real-time speed and can
  never drain a backlog. A startup stall therefore became a permanent delay.

The sender's own latency statistics measured scheduled playout downstream of
the stamping ("excludes upstream capture") and reported the streams as aligned
while they were not.

## Decision

- Every packet carries its capture time as PTS on `CLOCK_MONOTONIC`, in
  microseconds. Video stamps when the frame is acquired, before scaling and
  queueing. Audio derives packet times from its sample count and is corrected
  toward the observed time (`now - pa_simple_get_latency - duration`): errors
  under 40 ms are slewed to follow device drift, larger ones re-anchor, and
  time never moves backwards.
- The sender back-dates each live packet by its age
  (`now - capture time`) so both streams keep their true capture offset.
  Packets whose timestamps are negative or older than 30 s are not treated as
  capture-clock stamped and use the previous pacing, which keeps older helpers
  working. A single warning is logged when that happens.
- The audio capture stream requests 20 ms fragments and at most 200 ms of
  server-side buffering, so overload drops the oldest audio instead of adding
  delay.

## Consequences

- Audio/video alignment no longer depends on startup order or on how long each
  pipeline takes, and recovers from stalls.
- Helpers and sender must agree on the clock. The fork patch and the Rust
  helpers change together; the fallback only protects against old helpers.
- The `wf-recorder` screen path still emits its own timestamps, so screen video
  uses arrival-time stamping until that path is replaced or relayed. Window
  capture and application audio are fully on the shared clock.
- Receiver-side audio delay (for example an HDMI soundbar) is outside this
  model. A per-receiver audio offset would belong in the receiver profiles.
- Confirmed by listening on a Chromecast with Google TV after the change; a
  measured flash/beep offset is still required by the hardware checklist.
