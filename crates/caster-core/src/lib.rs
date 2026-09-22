//! Stable domain vocabulary and compile-time adapter contracts for Caster.

pub mod adapters;
pub mod api;
pub mod constants;
pub mod types;

pub use types::{
    AudioMode, BitrateChoice, Capabilities, Quality, QualityPreset, Receiver, Request,
    SessionState, Source, SourceKind, Status,
};
