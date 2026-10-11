//! Minimal safe ownership wrapper around PulseAudio's synchronous capture API.

use anyhow::{Context, Result};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::{self, NonNull},
};

const PA_STREAM_RECORD: c_int = 2;
const PA_SAMPLE_FLOAT32LE: c_int = 5;

/// `pa_buffer_attr`; `u32::MAX` selects the server default for a field.
#[repr(C)]
struct PaBufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

/// Capture fragment and the most audio the server may hold for us. The
/// server default is seconds of buffering, which becomes permanent A/V delay
/// whenever the consumer is briefly slow, so keep both small and let the
/// server drop the oldest audio instead.
const FRAGMENT_MS: u32 = 20;
const MAX_BUFFERED_MS: u32 = 200;

#[repr(C)]
struct PaSampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[link(name = "pulse-simple")]
unsafe extern "C" {
    fn pa_simple_new(
        server: *const c_char,
        name: *const c_char,
        direction: c_int,
        device: *const c_char,
        stream_name: *const c_char,
        sample_spec: *const PaSampleSpec,
        channel_map: *const c_void,
        buffer_attributes: *const c_void,
        error: *mut c_int,
    ) -> *mut c_void;
    fn pa_simple_free(stream: *mut c_void);
    fn pa_simple_read(
        stream: *mut c_void,
        data: *mut c_void,
        bytes: usize,
        error: *mut c_int,
    ) -> c_int;
    fn pa_simple_get_latency(stream: *mut c_void, error: *mut c_int) -> u64;
}

#[link(name = "pulse")]
unsafe extern "C" {
    fn pa_strerror(error: c_int) -> *const c_char;
}

fn error_message(error: c_int) -> String {
    // SAFETY: libpulse returns a process-lifetime NUL-terminated description
    // for every error code. A null return is handled defensively.
    unsafe {
        let value = pa_strerror(error);
        if value.is_null() {
            format!("PulseAudio error {error}")
        } else {
            CStr::from_ptr(value).to_string_lossy().into_owned()
        }
    }
}

pub struct Capture {
    stream: NonNull<c_void>,
}

impl Capture {
    pub fn connect(device: &str, sample_rate: u32, channels: u8) -> Result<Self> {
        anyhow::ensure!(!device.is_empty(), "Audio monitor name is empty");
        anyhow::ensure!(
            (8_000..=192_000).contains(&sample_rate),
            "Invalid sample rate"
        );
        anyhow::ensure!((1..=8).contains(&channels), "Invalid channel count");
        let application = CString::new("Caster")?;
        let stream_name = CString::new("Cast audio")?;
        let device = CString::new(device).context("Audio monitor contains a NUL byte")?;
        let specification = PaSampleSpec {
            format: PA_SAMPLE_FLOAT32LE,
            rate: sample_rate,
            channels,
        };
        let bytes_per_ms = sample_rate * u32::from(channels) * 4 / 1_000;
        let attributes = PaBufferAttr {
            maxlength: bytes_per_ms * MAX_BUFFERED_MS,
            tlength: u32::MAX,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: bytes_per_ms * FRAGMENT_MS,
        };
        let mut error = 0;
        // SAFETY: all pointers remain valid for the call, optional pointers
        // are null as required by the API, and the returned handle is uniquely
        // owned by this wrapper until Drop.
        let stream = unsafe {
            pa_simple_new(
                ptr::null(),
                application.as_ptr(),
                PA_STREAM_RECORD,
                device.as_ptr(),
                stream_name.as_ptr(),
                &specification,
                ptr::null(),
                (&raw const attributes).cast(),
                &mut error,
            )
        };
        Ok(Self {
            stream: NonNull::new(stream)
                .with_context(|| format!("Cannot capture audio: {}", error_message(error)))?,
        })
    }

    pub fn read_exact(&mut self, destination: &mut [u8]) -> Result<()> {
        anyhow::ensure!(!destination.is_empty(), "Audio read buffer is empty");
        let mut error = 0;
        // SAFETY: destination is writable for its declared length and the
        // stream handle stays valid for the duration of this blocking call.
        let result = unsafe {
            pa_simple_read(
                self.stream.as_ptr(),
                destination.as_mut_ptr().cast(),
                destination.len(),
                &mut error,
            )
        };
        anyhow::ensure!(
            result >= 0,
            "Audio capture failed: {}",
            error_message(error)
        );
        Ok(())
    }
}

impl Capture {
    /// Microseconds between the device capturing the audio the next read will
    /// return and now. `None` when the server cannot report it.
    pub fn latency_micros(&mut self) -> Option<i64> {
        let mut error = 0;
        // SAFETY: the stream handle is valid and error is writable.
        let latency = unsafe { pa_simple_get_latency(self.stream.as_ptr(), &mut error) };
        // pa_usec_t is unsigned; the API reports failure as (pa_usec_t)-1.
        if latency == u64::MAX {
            None
        } else {
            i64::try_from(latency).ok()
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns the non-null handle and Drop runs
        // exactly once.
        unsafe { pa_simple_free(self.stream.as_ptr()) };
    }
}
