# ADR 0001: Rust-first host pipeline

- Status: Accepted
- Date: 2026-09-22

## Context

The working prototype mixes Rust control code, a C Wayland helper, shell-facing
audio tools, FFmpeg subprocesses, QML, and a patched C++ Cast sender. Multiple
host-side processes and stringly boundaries make cleanup and extension harder.

## Decision

Implement capture, audio routing/capture, media framing, configuration,
supervision integration, and CLI control in Rust where mature system bindings
exist. QML remains the Omarchy UI. Open Screen remains the C++ Cast Streaming
engine because replacing that protocol implementation is a separate project.

## Consequences

The C helper, `wf-recorder`, repeated `pactl` polling, and FFmpeg subprocesses
can be retired after parity tests. Native FFmpeg, Pulse, Wayland, and systemd
libraries remain valid dependencies underneath Rust bindings.
