use core::fmt;

use hector_protocol::{ProtocolError, ProtocolMessage};

use crate::FakeLifecycle;

/// A required command-line flag.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CliFlag {
    /// The configured worker role.
    Role,
    /// The selected deterministic scenario.
    Scenario,
}

impl fmt::Display for CliFlag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Role => formatter.write_str("--role"),
            Self::Scenario => formatter.write_str("--scenario"),
        }
    }
}

/// Deterministic command-line configuration failures.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    /// A required flag was absent.
    MissingFlag(CliFlag),
    /// A flag appeared more than once.
    DuplicateFlag(CliFlag),
    /// A flag was not followed by a value.
    MissingValue(CliFlag),
    /// A token looked like an unsupported flag.
    UnknownFlag,
    /// The role value was not one of the three process-worker roles.
    UnknownRole,
    /// The scenario name was not built in.
    UnknownScenario,
    /// A positional, non-Unicode, or extra argument was supplied.
    UnexpectedArgument,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFlag(flag) => write!(formatter, "required flag {flag} is missing"),
            Self::DuplicateFlag(flag) => {
                write!(formatter, "flag {flag} was provided more than once")
            }
            Self::MissingValue(flag) => write!(formatter, "flag {flag} requires a separate value"),
            Self::UnknownFlag => formatter.write_str("an unknown command-line flag was provided"),
            Self::UnknownRole => formatter.write_str("worker role must be asr, llm, or tts"),
            Self::UnknownScenario => formatter.write_str("fake-worker scenario is unknown"),
            Self::UnexpectedArgument => {
                formatter.write_str("an unexpected command-line argument was provided")
            }
        }
    }
}

impl std::error::Error for CliError {}

/// Deterministic built-in script construction failures.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ScriptError {
    /// The requested scenario name is not built in.
    UnknownScenario,
    /// A stale-generation scenario cannot decrement the supplied epoch.
    InvalidGenerationBoundary,
    /// A request-mismatch scenario could not construct a distinct request.
    InvalidRequestBoundary,
    /// A built-in typed payload violated the H13 protocol contract.
    InvalidTypedPayload,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScenario => formatter.write_str("fake-worker scenario is unknown"),
            Self::InvalidGenerationBoundary => {
                formatter.write_str("scenario cannot alter this generation boundary")
            }
            Self::InvalidRequestBoundary => {
                formatter.write_str("scenario cannot construct a distinct request identity")
            }
            Self::InvalidTypedPayload => {
                formatter.write_str("built-in scenario payload violates the protocol contract")
            }
        }
    }
}

impl std::error::Error for ScriptError {}

/// Stable message classification used by lifecycle diagnostics.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MessageKind {
    Hello,
    Ready,
    Work,
    Output,
    Cancel,
    CancellationAcknowledged,
    Shutdown,
    Stopped,
    WorkerFault,
}

impl From<&ProtocolMessage> for MessageKind {
    fn from(message: &ProtocolMessage) -> Self {
        match message {
            ProtocolMessage::Hello { .. } => Self::Hello,
            ProtocolMessage::Ready { .. } => Self::Ready,
            ProtocolMessage::Work(_) => Self::Work,
            ProtocolMessage::Output(_) => Self::Output,
            ProtocolMessage::Cancel { .. } => Self::Cancel,
            ProtocolMessage::CancellationAcknowledged { .. } => Self::CancellationAcknowledged,
            ProtocolMessage::Shutdown => Self::Shutdown,
            ProtocolMessage::Stopped { .. } => Self::Stopped,
            ProtocolMessage::WorkerFault(_) => Self::WorkerFault,
        }
    }
}

/// Strict fake-worker lifecycle violations.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LifecycleError {
    /// A message is not accepted in the current lifecycle state.
    UnexpectedMessage {
        state: FakeLifecycle,
        message: MessageKind,
    },
    /// A hello or work payload targets a different worker role.
    WrongRole,
    /// A second work item arrived while one was active.
    WorkAlreadyActive,
    /// Cancellation was requested without active work.
    CancelWhileIdle,
    /// Cancellation did not match the exact active correlation.
    CorrelationMismatch,
    /// An adversarial scenario cannot operate at the supplied numeric boundary.
    ScenarioPrecondition,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedMessage { state, message } => {
                write!(formatter, "message {message:?} is invalid while {state:?}")
            }
            Self::WrongRole => formatter.write_str("message targets the wrong worker role"),
            Self::WorkAlreadyActive => formatter.write_str("worker already has active work"),
            Self::CancelWhileIdle => formatter.write_str("worker has no active request to cancel"),
            Self::CorrelationMismatch => {
                formatter.write_str("cancellation does not match active work")
            }
            Self::ScenarioPrecondition => {
                formatter.write_str("scenario precondition is not satisfied")
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

/// Top-level process execution failures without nondeterministic OS text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FakeWorkerError {
    Cli(CliError),
    Script(ScriptError),
    Lifecycle(LifecycleError),
    Protocol(ProtocolError),
    InputClosed(FakeLifecycle),
    StdinIo,
    StdoutIo,
}

impl FakeWorkerError {
    /// Returns the stable process exit code for this failure.
    pub const fn exit_code(&self) -> FakeWorkerExitCode {
        match self {
            Self::Cli(_) | Self::Script(_) => FakeWorkerExitCode::InvalidConfiguration,
            Self::Lifecycle(_) => FakeWorkerExitCode::LifecycleViolation,
            Self::Protocol(_) | Self::InputClosed(_) => FakeWorkerExitCode::ProtocolInputFailure,
            Self::StdinIo => FakeWorkerExitCode::StdinIoFailure,
            Self::StdoutIo => FakeWorkerExitCode::StdoutIoFailure,
        }
    }
}

impl fmt::Display for FakeWorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli(error) => write!(formatter, "configuration error: {error}"),
            Self::Script(error) => write!(formatter, "script error: {error}"),
            Self::Lifecycle(error) => write!(formatter, "lifecycle error: {error}"),
            Self::Protocol(error) => write!(formatter, "protocol input error: {error}"),
            Self::InputClosed(state) => {
                write!(
                    formatter,
                    "protocol input closed while worker was {state:?}"
                )
            }
            Self::StdinIo => formatter.write_str("stdin I/O failed"),
            Self::StdoutIo => formatter.write_str("stdout I/O failed"),
        }
    }
}

impl std::error::Error for FakeWorkerError {}

impl From<ProtocolError> for FakeWorkerError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LifecycleError> for FakeWorkerError {
    fn from(value: LifecycleError) -> Self {
        Self::Lifecycle(value)
    }
}

impl From<ScriptError> for FakeWorkerError {
    fn from(value: ScriptError) -> Self {
        Self::Script(value)
    }
}

/// Stable fake-worker process exit codes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum FakeWorkerExitCode {
    Success = 0,
    InvalidConfiguration = 2,
    LifecycleViolation = 3,
    ProtocolInputFailure = 4,
    StdinIoFailure = 5,
    StdoutIoFailure = 6,
    IntentionalCrash = 70,
}

impl FakeWorkerExitCode {
    /// Returns the numeric process exit code.
    pub const fn code(self) -> i32 {
        self as i32
    }
}
