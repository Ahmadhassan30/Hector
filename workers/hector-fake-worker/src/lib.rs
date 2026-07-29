//! Deterministic protocol-level fake ASR, LLM, and TTS workers.
//!
//! The fake worker exercises Hector's typed worker protocol without models,
//! device access, asynchronous execution, process supervision, or application
//! state. It deliberately contains hostile scenarios for later runtime tests.

mod error;
mod lifecycle;
mod script;
mod stdio;

pub use error::{
    CliError, CliFlag, FakeWorkerError, FakeWorkerExitCode, LifecycleError, MessageKind,
    ScriptError,
};
pub use lifecycle::{FakeLifecycle, FakeWorker};
pub use script::{
    FakeDisposition, FakeScenario, FakeWorkerConfig, FakeWorkerResult, MalformedFrameKind,
};
pub use stdio::{RunOutcome, run_from_environment};
