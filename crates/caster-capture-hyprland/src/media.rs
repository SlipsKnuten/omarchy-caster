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
    frame_number: i64,
}

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
            parameters.set_time_base(Rational(1, i32::try_from(fps)?));
            stream.set_parameters(&parameters);
            stream.set_time_base(Rational(1, i32::try_from(fps)?));
            stream.set_avg_frame_rate(Rational(i32::try_from(fps)?, 1));
            unsafe {
                (*stream.parameters().as_mut_ptr()).codec_tag =
                    ffmpeg::ffi::avcodec_pix_fmt_to_codec_tag(Pixel::YUV420P.into());
            }
        }
        output.write_header()?;
        Ok(Self {
            output,
            width,
            height,
            fps,
            frame_number: 0,
        })
    }

    pub fn write_frame(&mut self, source: &Frame) -> Result<()> {
        validate_frame(source)?;
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
        packet.set_pts(Some(self.frame_number));
        packet.set_dts(Some(self.frame_number));
        packet.set_duration(1);
        packet.set_stream(0);
        packet.set_position(-1);
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
}
