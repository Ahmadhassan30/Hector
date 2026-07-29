//! Callback-safe audio storage and observations for Hector.
//!
//! This crate owns bounded native-sample primitives only. Device streams,
//! protocol conversion, resampling, recovery policy, and reducer decisions
//! remain outside this boundary.

#[cfg(not(target_has_atomic = "64"))]
compile_error!("hector-audio requires lock-free 64-bit atomics");

mod control;
mod counters;
mod error;
mod spsc;

pub use control::{AudioEpochSlot, UrgentGenerationSlot};
pub use counters::{AudioCounterReader, AudioCounterSnapshot};
pub use error::{AudioBufferError, PopError, PushError};
pub use spsc::{AudioConsumer, AudioProducer, AudioSpscBuffer};
