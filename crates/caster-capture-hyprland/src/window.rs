use crate::protocol::hyprland_toplevel_export::{
    hyprland_toplevel_export_frame_v1::{self, HyprlandToplevelExportFrameV1},
    hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1,
};
use anyhow::{Context, Result, bail};
use memmap2::{MmapMut, MmapOptions};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{MemfdFlags, memfd_create},
};
use std::{
    fs::File,
    os::fd::AsFd,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer::WlBuffer,
        wl_registry::{self, WlRegistry},
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
    },
};

const MAX_DIMENSION: u32 = 16_384;
const MAX_STRIDE: u32 = 262_144;
const FRAME_TIMEOUT: Duration = Duration::from_secs(3);

fn trace(message: impl std::fmt::Display) {
    if std::env::var_os("CASTER_CAPTURE_TRACE").is_some() {
        eprintln!("caster-capture: {message}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Xrgb8888,
    Argb8888,
    Xbgr8888,
    Abgr8888,
}

impl PixelFormat {
    fn from_wayland(format: WEnum<wl_shm::Format>) -> Result<Self> {
        match format {
            WEnum::Value(wl_shm::Format::Xrgb8888) => Ok(Self::Xrgb8888),
            WEnum::Value(wl_shm::Format::Argb8888) => Ok(Self::Argb8888),
            WEnum::Value(wl_shm::Format::Xbgr8888) => Ok(Self::Xbgr8888),
            WEnum::Value(wl_shm::Format::Abgr8888) => Ok(Self::Abgr8888),
            _ => bail!("Unsupported Hyprland shared-memory pixel format"),
        }
    }
}

#[derive(Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub y_invert: bool,
    pub timestamp_ns: u128,
    pub data: Vec<u8>,
}

struct CaptureState {
    shm: Option<WlShm>,
    manager: Option<HyprlandToplevelExportManagerV1>,
    buffer: Option<WlBuffer>,
    mapping: Option<MmapMut>,
    width: u32,
    height: u32,
    stride: u32,
    format: Option<PixelFormat>,
    y_invert: bool,
    timestamp_ns: u128,
    ready: bool,
    failed: bool,
    error: Option<String>,
}

impl CaptureState {
    fn empty() -> Self {
        Self {
            shm: None,
            manager: None,
            buffer: None,
            mapping: None,
            width: 0,
            height: 0,
            stride: 0,
            format: None,
            y_invert: false,
            timestamp_ns: 0,
            ready: false,
            failed: false,
            error: None,
        }
    }

    fn reset_frame(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            buffer.destroy();
        }
        self.mapping = None;
        self.width = 0;
        self.height = 0;
        self.stride = 0;
        self.format = None;
        self.y_invert = false;
        self.timestamp_ns = 0;
        self.ready = false;
        self.failed = false;
        self.error = None;
    }

    fn allocate_buffer(
        &mut self,
        frame: &HyprlandToplevelExportFrameV1,
        qh: &QueueHandle<Self>,
    ) -> Result<()> {
        anyhow::ensure!(
            self.width > 0
                && self.height > 0
                && self.width <= MAX_DIMENSION
                && self.height <= MAX_DIMENSION
                && self.stride >= self.width.saturating_mul(4)
                && self.stride <= MAX_STRIDE,
            "Invalid window buffer dimensions"
        );
        let size = usize::try_from(self.stride)?
            .checked_mul(usize::try_from(self.height)?)
            .context("Window buffer is too large")?;
        anyhow::ensure!(size <= i32::MAX as usize, "Window buffer is too large");
        let fd = memfd_create("caster-window", MemfdFlags::CLOEXEC)?;
        rustix::fs::ftruncate(&fd, u64::try_from(size)?)?;
        let file = File::from(fd);
        let mapping = unsafe { MmapOptions::new().len(size).map_mut(&file)? };
        let shm = self.shm.as_ref().context("wl_shm is unavailable")?;
        let pool = shm.create_pool(file.as_fd(), i32::try_from(size)?, qh, ());
        let raw_format = match self.format.context("No shared-memory format offered")? {
            PixelFormat::Xrgb8888 => wl_shm::Format::Xrgb8888,
            PixelFormat::Argb8888 => wl_shm::Format::Argb8888,
            PixelFormat::Xbgr8888 => wl_shm::Format::Xbgr8888,
            PixelFormat::Abgr8888 => wl_shm::Format::Abgr8888,
        };
        let buffer = pool.create_buffer(
            0,
            i32::try_from(self.width)?,
            i32::try_from(self.height)?,
            i32::try_from(self.stride)?,
            raw_format,
            qh,
            (),
        );
        pool.destroy();
        frame.copy(&buffer, 1);
        self.buffer = Some(buffer);
        self.mapping = Some(mapping);
        Ok(())
    }
}

impl Dispatch<WlRegistry, ()> for CaptureState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_shm" => {
                    trace(format!("binding wl_shm v{}", version.min(1)));
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "hyprland_toplevel_export_manager_v1" => {
                    // The numeric window-address request is the stable v1 path.
                    // Version 2 only adds foreign-toplevel object handles.
                    trace("binding toplevel export v1");
                    state.manager = Some(registry.bind(name, 1, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<HyprlandToplevelExportFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        frame: &HyprlandToplevelExportFrameV1,
        event: hyprland_toplevel_export_frame_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        trace(format!("frame event: {event:?}"));
        match event {
            hyprland_toplevel_export_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => match PixelFormat::from_wayland(format) {
                Ok(format) => {
                    state.format = Some(format);
                    state.width = width;
                    state.height = height;
                    state.stride = stride;
                }
                Err(error) => state.error = Some(error.to_string()),
            },
            hyprland_toplevel_export_frame_v1::Event::Flags { flags } => {
                if let WEnum::Value(flags) = flags {
                    state.y_invert =
                        flags.contains(hyprland_toplevel_export_frame_v1::Flags::YInvert);
                }
            }
            hyprland_toplevel_export_frame_v1::Event::Ready {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => {
                let seconds = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                state.timestamp_ns = u128::from(seconds) * 1_000_000_000 + u128::from(tv_nsec);
                state.ready = true;
            }
            hyprland_toplevel_export_frame_v1::Event::Failed => {
                state.failed = true;
            }
            hyprland_toplevel_export_frame_v1::Event::BufferDone => {
                if state.error.is_none()
                    && let Err(error) = state.allocate_buffer(frame, qh)
                {
                    state.error = Some(format!("{error:#}"));
                }
            }
            hyprland_toplevel_export_frame_v1::Event::Damage { .. }
            | hyprland_toplevel_export_frame_v1::Event::LinuxDmabuf { .. } => {}
        }
    }
}

delegate_noop!(CaptureState: ignore WlShm);
delegate_noop!(CaptureState: ignore WlShmPool);
delegate_noop!(CaptureState: ignore WlBuffer);
delegate_noop!(CaptureState: ignore HyprlandToplevelExportManagerV1);

pub struct WindowCapturer {
    queue: EventQueue<CaptureState>,
    state: CaptureState,
    address: u32,
}

impl WindowCapturer {
    pub fn connect(address: &str) -> Result<Self> {
        let address = super::window_handle(address)?;
        let connection = Connection::connect_to_env().context("Cannot connect to Wayland")?;
        let mut queue = connection.new_event_queue();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let mut state = CaptureState::empty();
        queue.roundtrip(&mut state)?;
        anyhow::ensure!(state.shm.is_some(), "wl_shm is unavailable");
        anyhow::ensure!(
            state.manager.is_some(),
            "Hyprland toplevel export is unavailable"
        );
        Ok(Self {
            queue,
            state,
            address,
        })
    }

    pub fn next_frame(&mut self) -> Result<Frame> {
        self.state.reset_frame();
        let qh = self.queue.handle();
        let frame = self
            .state
            .manager
            .as_ref()
            .context("Hyprland toplevel export disappeared")?
            .capture_toplevel(1, self.address, &qh, ());
        trace(format!("requested window handle 0x{:x}", self.address));
        let deadline = Instant::now() + FRAME_TIMEOUT;
        while !self.state.ready && !self.state.failed && self.state.error.is_none() {
            self.queue.dispatch_pending(&mut self.state)?;
            if self.state.ready || self.state.failed || self.state.error.is_some() {
                break;
            }
            self.queue.flush()?;
            let Some(read_guard) = self.queue.prepare_read() else {
                continue;
            };
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("Hyprland capture timed out");
            }
            let timeout = Timespec {
                tv_sec: i64::try_from(remaining.as_secs())?,
                tv_nsec: i64::from(remaining.subsec_nanos()),
            };
            let ready = {
                let mut fds = [PollFd::new(&self.queue, PollFlags::IN)];
                poll(&mut fds, Some(&timeout))?
            };
            if ready == 0 {
                bail!("Hyprland capture timed out");
            }
            read_guard.read()?;
        }
        frame.destroy();
        if let Some(error) = self.state.error.take() {
            bail!(error);
        }
        anyhow::ensure!(
            !self.state.failed && self.state.ready,
            "Selected window is unavailable or sharing was revoked"
        );
        let mapping = self
            .state
            .mapping
            .take()
            .context("Capture returned no pixels")?;
        let format = self
            .state
            .format
            .context("Capture returned no pixel format")?;
        Ok(Frame {
            width: self.state.width,
            height: self.state.height,
            stride: self.state.stride,
            format,
            y_invert: self.state.y_invert,
            timestamp_ns: self.state.timestamp_ns,
            data: mapping.as_ref().to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_pixel_formats() {
        assert!(PixelFormat::from_wayland(WEnum::Unknown(0xffff_ffff)).is_err());
    }

    #[test]
    fn frame_data_is_owned() {
        let frame = Frame {
            width: 1,
            height: 1,
            stride: 4,
            format: PixelFormat::Argb8888,
            y_invert: false,
            timestamp_ns: 1,
            data: vec![0, 0, 0, 0],
        };
        assert_eq!(
            frame.data.len(),
            frame.stride as usize * frame.height as usize
        );
    }
}
