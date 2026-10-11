//! FFmpeg-backed scaling and NUT framing with ownership kept on the Rust side.

use crate::window::{Frame, PixelFormat};
use anyhow::{Context as _, Result};
use ffmpeg::{
    Packet, Rational, codec, encoder, format,
    frame::Video,
    software::scaling::{context::Context, flag::Flags},
    util::format::Pixel,
};
use ffmpeg_next as ffmpeg;
use std::path::Path;

pub struct NutVideoWriter {
    output: format::context::Output,
    width: u32,
    height: u32,
    fps: u32,
    output_time_base: Rational,
    frame_number: i64,
    last_pts_us: Option<i64>,
}

/// Stream time base. Packets carry capture time on the shared monotonic clock
/// so the sender can align video with audio captured by another process.
const TIME_BASE: Rational = Rational(1, 1_000_000);

impl NutVideoWriter {
    pub fn open(path: impl AsRef<Path>, width: u32, height: u32, fps: u32) -> Result<Self> {
        anyhow::ensure!(
            width >= 2
                && height >= 2
                && width <= 3840
                && height <= 2160
                && width.is_multiple_of(2)
                && height.is_multiple_of(2),
            "Invalid output dimensions"
        );
        anyhow::ensure!((1..=60).contains(&fps), "Invalid frame rate");
        ffmpeg::init()?;
        let mut output = format::output_as(path.as_ref(), "nut")?;
        let raw = encoder::find(codec::Id::RAWVIDEO).context("FFmpeg rawvideo is unavailable")?;
        {
            let mut stream = output.add_stream(Some(raw))?;
            let mut parameters = codec::context::Context::new_with_codec(raw)
                .encoder()
                .video()?;
            parameters.set_width(width);
            parameters.set_height(height);
            parameters.set_format(Pixel::YUV420P);
            parameters.set_aspect_ratio(Rational(1, 1));
            parameters.set_frame_rate(Some(Rational(i32::try_from(fps)?, 1)));
            parameters.set_time_base(TIME_BASE);
            stream.set_parameters(&parameters);
            stream.set_time_base(TIME_BASE);
            stream.set_avg_frame_rate(Rational(i32::try_from(fps)?, 1));
            unsafe {
                (*stream.parameters().as_mut_ptr()).codec_tag =
                    ffmpeg::ffi::avcodec_pix_fmt_to_codec_tag(Pixel::YUV420P.into());
            }
        }
        output.write_header()?;
        let output_time_base = output
            .stream(0)
            .context("NUT output contains no video stream")?
            .time_base();
        Ok(Self {
            output,
            width,
            height,
            fps,
            output_time_base,
            frame_number: 0,
            last_pts_us: None,
        })
    }

    /// Write a frame at a synthetic, perfectly paced timestamp.
    pub fn write_frame(&mut self, source: &Frame) -> Result<()> {
        let pts_us = self.frame_number * 1_000_000 / i64::from(self.fps);
        self.write_frame_at(source, pts_us)
    }

    /// Write a frame stamped with its capture time in microseconds on
    /// `CLOCK_MONOTONIC`. Timestamps are forced strictly increasing.
    pub fn write_frame_at(&mut self, source: &Frame, capture_us: i64) -> Result<()> {
        validate_frame(source)?;
        let pts_us = self
            .last_pts_us
            .map_or(capture_us, |last| capture_us.max(last + 1));
        self.last_pts_us = Some(pts_us);
        let source_pixel = match source.format {
            PixelFormat::Xrgb8888 | PixelFormat::Argb8888 => Pixel::BGRA,
            PixelFormat::Xbgr8888 | PixelFormat::Abgr8888 => Pixel::RGBA,
        };
        let mut input = Video::new(source_pixel, source.width, source.height);
        let destination_stride = input.stride(0);
        let row_bytes = usize::try_from(source.width)? * 4;
        let source_stride = usize::try_from(source.stride)?;
        let height = usize::try_from(source.height)?;
        for destination_y in 0..height {
            let source_y = if source.y_invert {
                height - 1 - destination_y
            } else {
                destination_y
            };
            let source_start = source_y * source_stride;
            let destination_start = destination_y * destination_stride;
            input.data_mut(0)[destination_start..destination_start + row_bytes]
                .copy_from_slice(&source.data[source_start..source_start + row_bytes]);
        }

        let (scaled_width, scaled_height, left, top) =
            fit_canvas(source.width, source.height, self.width, self.height)?;
        let mut scaled = Video::new(Pixel::YUV420P, scaled_width, scaled_height);
        let mut scaler = Context::get(
            source_pixel,
            source.width,
            source.height,
            Pixel::YUV420P,
            scaled_width,
            scaled_height,
            Flags::BILINEAR,
        )?;
        scaler.run(&input, &mut scaled)?;

        let mut canvas = Video::new(Pixel::YUV420P, self.width, self.height);
        canvas.data_mut(0).fill(16);
        canvas.data_mut(1).fill(128);
        canvas.data_mut(2).fill(128);
        copy_yuv420(&scaled, &mut canvas, left, top)?;
        let payload = pack_yuv420(&canvas)?;
        let mut packet = Packet::copy(&payload);
        packet.set_pts(Some(pts_us));
        packet.set_dts(Some(pts_us));
        packet.set_duration(1_000_000 / i64::from(self.fps));
        packet.set_stream(0);
        packet.set_position(-1);
        packet.rescale_ts(TIME_BASE, self.output_time_base);
        packet.write(&mut self.output)?;
        self.frame_number += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.output.write_trailer()?;
        Ok(())
    }

    pub fn fps(&self) -> u32 {
        self.fps
    }
}

fn validate_frame(frame: &Frame) -> Result<()> {
    anyhow::ensure!(frame.width > 0 && frame.height > 0, "Empty source frame");
    let row_bytes = usize::try_from(frame.width)?
        .checked_mul(4)
        .context("Source row is too large")?;
    anyhow::ensure!(
        usize::try_from(frame.stride)? >= row_bytes,
        "Invalid source stride"
    );
    let required = usize::try_from(frame.stride)?
        .checked_mul(usize::try_from(frame.height)?)
        .context("Source frame is too large")?;
    anyhow::ensure!(frame.data.len() >= required, "Truncated source frame");
    Ok(())
}

fn fit_canvas(
    source_width: u32,
    source_height: u32,
    canvas_width: u32,
    canvas_height: u32,
) -> Result<(u32, u32, u32, u32)> {
    anyhow::ensure!(source_width > 0 && source_height > 0, "Empty source frame");
    let mut width = canvas_width;
    let mut height = u32::try_from(
        u64::from(source_height) * u64::from(canvas_width) / u64::from(source_width),
    )?;
    if height > canvas_height {
        height = canvas_height;
        width = u32::try_from(
            u64::from(source_width) * u64::from(canvas_height) / u64::from(source_height),
        )?;
    }
    width -= width % 2;
    height -= height % 2;
    anyhow::ensure!(width >= 2 && height >= 2, "Unsupported source aspect ratio");
    let left = ((canvas_width - width) / 2) & !1;
    let top = ((canvas_height - height) / 2) & !1;
    Ok((width, height, left, top))
}

fn copy_yuv420(source: &Video, destination: &mut Video, left: u32, top: u32) -> Result<()> {
    for plane in 0..3 {
        let divisor = if plane == 0 { 1 } else { 2 };
        let width = usize::try_from(source.width() / divisor)?;
        let height = usize::try_from(source.height() / divisor)?;
        let left = usize::try_from(left / divisor)?;
        let top = usize::try_from(top / divisor)?;
        let source_stride = source.stride(plane);
        let destination_stride = destination.stride(plane);
        let source_data = source.data(plane);
        let destination_data = destination.data_mut(plane);
        for row in 0..height {
            let source_start = row * source_stride;
            let destination_start = (top + row) * destination_stride + left;
            destination_data[destination_start..destination_start + width]
                .copy_from_slice(&source_data[source_start..source_start + width]);
        }
    }
    Ok(())
}

fn pack_yuv420(frame: &Video) -> Result<Vec<u8>> {
    let capacity = usize::try_from(frame.width())?
        .checked_mul(usize::try_from(frame.height())?)
        .and_then(|pixels| pixels.checked_mul(3))
        .map(|bytes| bytes / 2)
        .context("Output frame is too large")?;
    let mut output = Vec::with_capacity(capacity);
    for plane in 0..3 {
        let width = usize::try_from(frame.plane_width(plane))?;
        let height = usize::try_from(frame.plane_height(plane))?;
        let stride = frame.stride(plane);
        let data = frame.data(plane);
        for row in 0..height {
            let start = row * stride;
            output.extend_from_slice(&data[start..start + width]);
        }
    }
    anyhow::ensure!(output.len() == capacity, "Unexpected YUV frame size");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn aspect_ratio_is_preserved_and_offsets_are_even() {
        assert_eq!(
            fit_canvas(1000, 1000, 1920, 1080).unwrap(),
            (1080, 1080, 420, 0)
        );
        assert_eq!(
            fit_canvas(640, 360, 1920, 1080).unwrap(),
            (1920, 1080, 0, 0)
        );
        assert_eq!(
            fit_canvas(360, 640, 1920, 1080).unwrap(),
            (606, 1080, 656, 0)
        );
    }

    #[test]
    fn rejects_truncated_frames() {
        let frame = Frame {
            width: 2,
            height: 2,
            stride: 8,
            format: PixelFormat::Argb8888,
            y_invert: false,
            timestamp_ns: 0,
            data: vec![0; 8],
        };
        assert!(validate_frame(&frame).is_err());
    }

    #[test]
    fn supported_presets_decode_with_real_time_timestamps() {
        ffmpeg::init().unwrap();
        let source = Frame {
            width: 4,
            height: 4,
            stride: 16,
            format: PixelFormat::Argb8888,
            y_invert: false,
            timestamp_ns: 0,
            data: vec![0x80; 64],
        };
        for (width, height) in [(1280, 720), (1920, 1080)] {
            let path = std::env::temp_dir().join(format!(
                "caster-video-{}-{width}x{height}.nut",
                std::process::id()
            ));
            let mut writer = NutVideoWriter::open(&path, width, height, 30).unwrap();
            for _ in 0..3 {
                writer.write_frame(&source).unwrap();
            }
            writer.finish().unwrap();

            let mut input = format::input(&path).unwrap();
            let stream = input.stream(0).unwrap();
            let time_base = stream.time_base();
            let mut decoder = codec::context::Context::from_parameters(stream.parameters())
                .unwrap()
                .decoder()
                .video()
                .unwrap();
            assert_eq!((decoder.width(), decoder.height()), (width, height));
            assert_eq!(decoder.format(), Pixel::YUV420P);
            let mut timestamps = Vec::new();
            let mut decoded = 0;
            for (_, packet) in input.packets() {
                timestamps.push(packet.pts().unwrap());
                decoder.send_packet(&packet).unwrap();
                let mut frame = Video::empty();
                while decoder.receive_frame(&mut frame).is_ok() {
                    decoded += 1;
                }
            }
            assert_eq!(decoded, 3);
            assert_eq!(timestamps.len(), 3);
            let tick = timestamps[1] - timestamps[0];
            let seconds = f64::from(i32::try_from(tick).unwrap())
                * f64::from(time_base.numerator())
                / f64::from(time_base.denominator());
            // Microsecond time base: allow one tick of rounding.
            assert!((seconds - 1.0 / 30.0).abs() < 2e-6);
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn capture_timestamps_are_preserved_and_monotonic() {
        ffmpeg::init().unwrap();
        let source = Frame {
            width: 4,
            height: 4,
            stride: 16,
            format: PixelFormat::Argb8888,
            y_invert: false,
            timestamp_ns: 0,
            data: vec![0x80; 64],
        };
        let path = std::env::temp_dir().join(format!("caster-video-at-{}.nut", std::process::id()));
        let mut writer = NutVideoWriter::open(&path, 1280, 720, 30).unwrap();
        // The last capture time goes backwards and must be nudged forward.
        for capture_us in [5_000_000_000, 5_000_040_000, 5_000_030_000] {
            writer.write_frame_at(&source, capture_us).unwrap();
        }
        writer.finish().unwrap();
        let mut input = format::input(&path).unwrap();
        let time_base = input.stream(0).unwrap().time_base();
        assert_eq!(
            (time_base.numerator(), time_base.denominator()),
            (1, 1_000_000)
        );
        let timestamps: Vec<i64> = input.packets().map(|(_, p)| p.pts().unwrap()).collect();
        assert_eq!(timestamps, [5_000_000_000, 5_000_040_000, 5_000_040_001]);
        fs::remove_file(path).unwrap();
    }
}
