//! Native Pulse-compatible capture and NUT framing for Caster.

mod clock;
pub mod media;
mod pulse;

use anyhow::Result;
use media::NutAudioWriter;
use pulse::Capture;
use std::path::Path;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u8 = 2;
pub const PACKET_SAMPLES: usize = 960;
const PACKET_DURATION_US: i64 = PACKET_SAMPLES as i64 * 1_000_000 / SAMPLE_RATE as i64;

pub fn stream_monitor(device: &str) -> Result<()> {
    let mut capture = Capture::connect(device, SAMPLE_RATE, CHANNELS)?;
    let mut output = NutAudioWriter::open(Path::new("pipe:1"), SAMPLE_RATE, CHANNELS)?;
    let mut packet = vec![0_u8; PACKET_SAMPLES * usize::from(CHANNELS) * size_of::<f32>()];
    loop {
        capture.read_exact(&mut packet)?;
        // Latency after the read is the age of the audio the next read will
        // return, which is also the end time of the packet just consumed.
        let observed_start = capture
            .latency_micros()
            .map(|latency| clock::monotonic_micros() - latency - PACKET_DURATION_US);
        output.write_interleaved_f32_at(&packet, observed_start)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_duration_is_twenty_milliseconds() {
        assert_eq!(PACKET_SAMPLES * 1_000 / SAMPLE_RATE as usize, 20);
    }
}
