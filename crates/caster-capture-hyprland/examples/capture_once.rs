use anyhow::{Context, Result};
use omarchy_caster_capture_hyprland::window::WindowCapturer;

fn main() -> Result<()> {
    let address = std::env::args()
        .nth(1)
        .context("usage: capture_once 0xADDRESS")?;
    let mut capture = WindowCapturer::connect(&address)?;
    let frame = capture.next_frame()?;
    println!(
        "{}x{} stride={} format={:?} inverted={} timestamp_ns={} bytes={}",
        frame.width,
        frame.height,
        frame.stride,
        frame.format,
        frame.y_invert,
        frame.timestamp_ns,
        frame.data.len()
    );
    Ok(())
}
