use anyhow::{Context, Result};
use omarchy_caster_capture_hyprland::{
    media::NutVideoWriter,
    window::{Frame, PixelFormat},
};

fn main() -> Result<()> {
    let output = std::env::args()
        .nth(1)
        .context("usage: synthetic_nut OUTPUT")?;
    let mut writer = NutVideoWriter::open(output, 1280, 720, 30)?;
    for index in 0..3u8 {
        let mut data = vec![0; 640 * 360 * 4];
        for pixel in data.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[40 + index * 20, 80, 180, 255]);
        }
        writer.write_frame(&Frame {
            width: 640,
            height: 360,
            stride: 640 * 4,
            format: PixelFormat::Argb8888,
            y_invert: false,
            timestamp_ns: u128::from(index) * 33_333_333,
            data,
        })?;
    }
    writer.finish()
}
