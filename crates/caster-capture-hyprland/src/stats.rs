//! Per-interval capture telemetry.
//!
//! One line per interval on stderr, which the supervisor appends to the session
//! log. It carries only counts and timings: never window titles or content.

use std::time::Duration;

/// How often a line is emitted.
pub const REPORT_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct IntervalStats {
    captured: u64,
    replaced: u64,
    written: u64,
    wait_us_sum: u64,
    wait_us_max: u64,
    write_us_sum: u64,
    write_us_max: u64,
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

impl IntervalStats {
    /// A frame arrived from the compositor after `wait` (request to ready).
    pub fn record_capture(&mut self, wait: Duration) {
        let wait = micros(wait);
        self.captured += 1;
        self.wait_us_sum += wait;
        self.wait_us_max = self.wait_us_max.max(wait);
    }

    /// A pending frame was overwritten before the writer took it.
    pub fn record_replaced(&mut self) {
        self.replaced += 1;
    }

    /// A frame was scaled, packed and written to the output pipe in `took`.
    pub fn record_write(&mut self, took: Duration) {
        let took = micros(took);
        self.written += 1;
        self.write_us_sum += took;
        self.write_us_max = self.write_us_max.max(took);
    }

    /// Format and reset the interval. `elapsed` is the wall time covered.
    pub fn take_report(&mut self, elapsed: Duration) -> String {
        let seconds = elapsed.as_secs_f64().max(0.001);
        let mean_ms = |sum: u64, count: u64| {
            if count == 0 {
                0.0
            } else {
                sum as f64 / count as f64 / 1_000.0
            }
        };
        let line = format!(
            "capture-stats: {:.1} fps captured, {:.1} fps written, {} replaced, \
             compositor wait {:.1}/{:.0} ms (mean/max), convert+write {:.1}/{:.0} ms",
            self.captured as f64 / seconds,
            self.written as f64 / seconds,
            self.replaced,
            mean_ms(self.wait_us_sum, self.captured),
            self.wait_us_max as f64 / 1_000.0,
            mean_ms(self.write_us_sum, self.written),
            self.write_us_max as f64 / 1_000.0,
        );
        *self = Self::default();
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_summarises_and_resets_the_interval() {
        let mut stats = IntervalStats::default();
        for wait in [10, 20, 30] {
            stats.record_capture(Duration::from_millis(wait));
        }
        stats.record_replaced();
        stats.record_write(Duration::from_millis(4));
        stats.record_write(Duration::from_millis(8));

        let line = stats.take_report(Duration::from_secs(2));
        assert_eq!(
            line,
            "capture-stats: 1.5 fps captured, 1.0 fps written, 1 replaced, \
             compositor wait 20.0/30 ms (mean/max), convert+write 6.0/8 ms"
        );
        assert_eq!(
            stats.take_report(Duration::from_secs(2)),
            "capture-stats: 0.0 fps captured, 0.0 fps written, 0 replaced, \
             compositor wait 0.0/0 ms (mean/max), convert+write 0.0/0 ms"
        );
    }
}
