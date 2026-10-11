use anyhow::Result;
use omarchy_caster_audio_pulse::{CHANNELS, PACKET_SAMPLES, SAMPLE_RATE, media::NutAudioWriter};
use std::{env, f32::consts::TAU};

fn main() -> Result<()> {
    let path = env::args().nth(1).unwrap_or_else(|| "audio.nut".into());
    let mut writer = NutAudioWriter::open(path, SAMPLE_RATE, CHANNELS)?;
    for packet in 0..10 {
        let mut bytes = Vec::with_capacity(PACKET_SAMPLES * usize::from(CHANNELS) * 4);
        for sample in 0..PACKET_SAMPLES {
            let index = packet * PACKET_SAMPLES + sample;
            let value = (TAU * 440.0 * index as f32 / SAMPLE_RATE as f32).sin() * 0.1;
            for _ in 0..CHANNELS {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        writer.write_interleaved_f32(&bytes)?;
    }
    writer.finish()
}
