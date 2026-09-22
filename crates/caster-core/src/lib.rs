//! Stable domain vocabulary and compile-time adapter contracts for Caster.

pub mod adapters;
pub mod api;
pub mod constants;
pub mod receiver_profiles;
pub mod types;

pub use types::{
    AudioMode, BackendInfo, BackendKind, BitrateChoice, Capabilities, CapabilityReport, Quality,
    QualityPreset, Receiver, Request, SessionState, Source, SourceKind, Status,
};
