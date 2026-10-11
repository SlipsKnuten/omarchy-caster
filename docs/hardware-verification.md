# Hardware verification record

Copy this section for each tested receiver and do not include private receiver
IDs, addresses, or window titles.

## Receiver

- Model:
- Firmware/platform version:
- Host Caster commit:
- Open Screen upstream/fork commit:
- Date:

## Modes

- [ ] Window video
- [ ] Screen video
- [ ] Application audio isolation
- [ ] System audio
- [ ] Auto quality
- [ ] 720p30
- [ ] 1080p30

## Measurements

- Visible end-to-end latency:
- Flash/beep A/V offset:
- Resolved quality and bitrate:
- CPU and memory after 10 minutes:
- CPU and memory after 60 minutes:

## Privacy and recovery

- [ ] Occluding window is absent from the TV.
- [ ] Resize, minimize, workspace movement, and close behave safely.
- [ ] Unrelated application audio remains local.
- [ ] Stop/failure removes all owned processes and temporary audio routes.
- [ ] Ten reconnect cycles complete without stale receiver sessions.

## Notes

Describe artifacts, receiver UI, failures, and any model-specific workaround.
- 2026-10-11: after ADR 0006, audio delay reported fixed by listening on the
  Chromecast with Google TV (window source, application audio). Not a measured
  flash/beep offset; the checklist item above remains open.
- 2026-10-11: window cast to a Chromecast (Master Bedroom TV) with application
  audio streamed. Captured frame rate was about 23-24 fps with the window on a
  hidden workspace and about 10-12 fps with it visible and focused; see
  docs/performance.md. CPU is high; power draw not yet measured in watts.
