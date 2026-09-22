//! Versioned machine-readable CLI envelope.

use crate::constants::API_VERSION;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response<T: Serialize> {
    Success { api: u32, ok: bool, result: T },
    Error { api: u32, ok: bool, error: ApiError },
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl<T: Serialize> Response<T> {
    pub fn success(result: T) -> Self {
        Self::Success {
            api: API_VERSION,
            ok: true,
            result,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_have_stable_keys() {
        let success = serde_json::to_value(Response::success(42)).unwrap();
        assert_eq!(success["api"], API_VERSION);
        assert_eq!(success["ok"], true);
        assert_eq!(success["result"], 42);
        let error = serde_json::to_value(Response::<()>::error("test", "message")).unwrap();
        assert_eq!(error["error"]["code"], "test");
        assert_eq!(error["error"]["message"], "message");
    }
}
