//! Compile-time extension points. Implementations live in focused adapter crates.

use crate::{AudioMode, Capabilities, Quality, Receiver, Source, Status};

pub type AdapterResult<T> = Result<T, AdapterError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterError {
    pub code: &'static str,
    pub message: String,
}

impl AdapterError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub trait ReceiverDiscovery: Send + Sync {
    fn id(&self) -> &'static str;
    fn discover(&self) -> AdapterResult<Vec<Receiver>>;
}

pub trait CaptureBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn sources(&self) -> AdapterResult<Vec<Source>>;
    fn validate(&self, source: &Source) -> AdapterResult<()>;
}

pub trait AudioBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, source: &Source, mode: AudioMode) -> bool;
    fn recover(&self) -> AdapterResult<()>;
}

pub trait TransportBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, receiver: &Receiver) -> bool;
    fn validate_quality(&self, quality: Quality) -> AdapterResult<()>;
}

pub trait SessionSupervisor: Send + Sync {
    fn id(&self) -> &'static str;
    fn is_active(&self) -> AdapterResult<bool>;
    fn stop(&self) -> AdapterResult<Status>;
}
