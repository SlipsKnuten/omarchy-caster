//! Shared capture clock. The audio and video helpers are separate processes,
//! so both stamp media with `CLOCK_MONOTONIC` microseconds.

use rustix::time::{ClockId, clock_gettime};

pub fn monotonic_micros() -> i64 {
    let now = clock_gettime(ClockId::Monotonic);
    now.tv_sec * 1_000_000 + now.tv_nsec / 1_000
}
