//! Versioned machine-readable CLI envelope.

use crate::constants::API_VERSION;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response<T: Serialize> {
    Success { api: u32, ok: bool, data: T },
    Error { api: u32, ok: bool, error: ApiError },
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl<T: Serialize> Response<T> {
    pub fn success(data: T) -> Self {
        Self::Success {
            api: API_VERSION,
            ok: true,
            data,
        }
    }

    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self::Error {
            api: API_VERSION,
            ok: false,
            error: ApiError {
                code,
                message: message.into(),
            },
        }
    }
}
