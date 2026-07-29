//! Windows-native worker process supervision for Hector.
//!
//! This crate owns process creation, redirected standard streams, Windows Job
//! Objects, bounded restart mechanics, and process-tree cleanup. It does not
//! own application state, request deadlines, freshness, or model behavior.

#![cfg(windows)]

mod config;
mod error;
mod handle;
mod job;
mod process;
mod stdio;
mod supervisor;

pub use config::{RestartPolicy, SupervisorTimeouts, WorkerProcessConfig};
pub use error::{
    CleanupFailure, ConfigError, ConfigField, HandshakeFailure, PipeStream, PlatformFailure,
    RestartCause, ShutdownProtocolFailure, SupervisorFailure, SupervisorFailureKind, ThreadKind,
    TimeoutKind, Win32Operation,
};
pub use supervisor::{
    DiagnosticSnapshot, ForcedTerminationReason, ForcedTerminationReport, ReceiveOutcome,
    RestartReport, ShutdownOutcome, ShutdownReport, SupervisorEvent, SupervisorState,
    TerminationOutcome, WorkerSupervisor,
};
