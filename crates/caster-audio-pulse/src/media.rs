//! Raw float PCM in a low-latency NUT stream for the Open Screen boundary.

use anyhow::{Context, Result};
use ffmpeg::{
    ChannelLayout, Packet, Rational, codec, encoder, format,
    util::format::{Sample, sample::Type},
};
use ffmpeg_next as ffmpeg;
use std::path::Path;

/// Differences larger than this are treated as a stall or burst and re-anchor
/// the clock immediately; smaller ones are slewed out to follow device drift.
const SNAP_THRESHOLD_US: i64 = 40_000;
const SLEW_DIVISOR: i64 = 32;

/// Maps a continuous sample count onto the shared monotonic clock.
///
/// Packet start times are derived from sample counts, so they are perfectly
/// regular, and only corrected toward observed capture times. Observed times
/// alone would jitter and could even reorder when the server delivers bursts.
struct SampleClock {
    sample_rate: i64,
    anchor_us: Option<i64>,
    samples: i64,
    last_start_us: Option<i64>,
}

impl SampleClock {
    const fn new(sample_rate: i64) -> Self {
        Self {
            sample_rate,
            anchor_us: None,
            samples: 0,
            last_start_us: None,
        }
    }

    /// Capture time in microseconds of the next packet's first sample, given
    /// the time it was observed to start (or `None` to trust the count).
    fn next_start(&mut self, observed_us: Option<i64>, packet_samples: i64) -> i64 {
        let offset = self.samples * 1_000_000 / self.sample_rate;
        if let Some(observed) = observed_us {
            match self.anchor_us {
                None => self.anchor_us = Some(observed - offset),
                Some(anchor) => {
                    let error = observed - (anchor + offset);
                    let step = if error.abs() > SNAP_THRESHOLD_US {
                        error
                    } else {
                        error / SLEW_DIVISOR
                    };
                    self.anchor_us = Some(anchor + step);
                }
            }
        }
        let anchor = *self.anchor_us.get_or_insert(0);
        let mut start = anchor + offset;
        if let Some(last) = self.last_start_us
            && start <= last
        {
            // A backwards snap must not reorder; keep time moving and carry
            // the correction into the anchor.
            start = last + 1;
            self.anchor_us = Some(start - offset);
        }
        self.last_start_us = Some(start);
        self.samples += packet_samples;
        start
    }
}

pub struct NutAudioWriter {
    output: format::context::Output,
    sample_rate: u32,
    channels: u8,
    clock: SampleClock,
}

impl NutAudioWriter {
    pub fn open(path: impl AsRef<Path>, sample_rate: u32, channels: u8) -> Result<Self> {
        anyhow::ensure!(
            (8_000..=192_000).contains(&sample_rate),
            "Invalid sample rate"
        );
        anyhow::ensure!(channels == 2, "Caster currently supports stereo audio");
        ffmpeg::init()?;
        let mut output = format::output_as(path.as_ref(), "nut")?;
        let codec = encoder::find(codec::Id::PCM_F32LE)
            .context("FFmpeg float PCM encoder is unavailable")?;
        {
            let mut stream = output.add_stream(Some(codec))?;
            let mut parameters = codec::context::Context::new_with_codec(codec)
                .encoder()
                .audio()?;
            parameters.set_rate(i32::try_from(sample_rate)?);
            parameters.set_format(Sample::F32(Type::Packed));
            parameters.set_channel_layout(ChannelLayout::STEREO);
            parameters.set_time_base(Rational(1, i32::try_from(sample_rate)?));
            stream.set_parameters(&parameters);
            stream.set_time_base(Rational(1, i32::try_from(sample_rate)?));
        }
        output.write_header()?;
        Ok(Self {
            output,
            sample_rate,
            channels,
            clock: SampleClock::new(i64::from(sample_rate)),
        })
    }

    /// Write a packet paced purely by its sample count.
    pub fn write_interleaved_f32(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_interleaved_f32_at(bytes, None)
    }

    /// Write a packet whose first sample was captured at `observed_start_us`
    /// on `CLOCK_MONOTONIC`.
    pub fn write_interleaved_f32_at(
        &mut self,
        bytes: &[u8],
        observed_start_us: Option<i64>,
    ) -> Result<()> {
        let frame_bytes = usize::from(self.channels) * size_of::<f32>();
        anyhow::ensure!(
            !bytes.is_empty() && bytes.len().is_multiple_of(frame_bytes),
            "PCM packet is not aligned to complete sample frames"
        );
        let samples = i64::try_from(bytes.len() / frame_bytes)?;
        let mut packet = Packet::copy(bytes);
        let start_us = self.clock.next_start(observed_start_us, samples);
        // Stream time base is 1/sample_rate; absolute capture time keeps the
        // audio comparable with video written by another process.
        let pts = i64::try_from(i128::from(start_us) * i128::from(self.sample_rate) / 1_000_000)?;
        packet.set_pts(Some(pts));
        packet.set_dts(Some(pts));
        packet.set_duration(samples);
        packet.set_stream(0);
        packet.set_position(-1);
        packet.write(&mut self.output)?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.output.write_trailer()?;
        Ok(())
    }

    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: i64 = 48_000;
    const PACKET: i64 = 960;

    #[test]
    fn regular_delivery_is_perfectly_paced() {
        let mut clock = SampleClock::new(RATE);
        let starts: Vec<i64> = (0..4)
            .map(|n| clock.next_start(Some(1_000_000 + n * 20_000), PACKET))
            .collect();
        assert_eq!(starts, [1_000_000, 1_020_000, 1_040_000, 1_060_000]);
    }

    #[test]
    fn delivery_jitter_does_not_reorder_or_bunch_timestamps() {
        let mut clock = SampleClock::new(RATE);
        // Bursty reads: observed starts are accurate but arrive unevenly.
        let observed = [0, 20_000, 40_000, 60_000];
        let starts: Vec<i64> = observed
            .iter()
            .map(|&o| clock.next_start(Some(o + 5_000_000), PACKET))
            .collect();
        assert!(starts.windows(2).all(|w| w[1] - w[0] == 20_000));
    }

    #[test]
    fn stall_reanchors_to_capture_time() {
        let mut clock = SampleClock::new(RATE);
        clock.next_start(Some(0), PACKET);
        clock.next_start(Some(20_000), PACKET);
        // One second of silence was never delivered.
        let resumed = clock.next_start(Some(1_040_000), PACKET);
        assert_eq!(resumed, 1_040_000);
    }

    #[test]
    fn device_drift_is_slewed_without_jumps() {
        let mut clock = SampleClock::new(RATE);
        // Device runs 0.1% fast: observed packets are 19.98 ms apart.
        let mut previous = clock.next_start(Some(0), PACKET);
        let mut worst_step = 0;
        for n in 1..2_000 {
            let start = clock.next_start(Some(n * 19_980), PACKET);
            worst_step = worst_step.max((start - previous - 20_000).abs());
            previous = start;
        }
        assert!(worst_step < 1_000, "step error {worst_step} us");
        // Tracked time ends close to the observed time, not 40 ms behind.
        assert!((previous - 1_999 * 19_980).abs() < 2_000);
    }

    #[test]
    fn backwards_snap_keeps_time_monotonic() {
        let mut clock = SampleClock::new(RATE);
        clock.next_start(Some(1_000_000), PACKET);
        let second = clock.next_start(Some(1_020_000), PACKET);
        let third = clock.next_start(Some(500_000), PACKET);
        assert!(third > second);
    }
}
