use std::{error::Error, fmt};

use hector_protocol::ProtocolError;

use crate::DiagnosticSnapshot;

/// Configuration field containing an invalid embedded NUL.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConfigField {
    Executable,
    Argument,
    WorkingDirectory,
}

/// One bounded supervisor timeout class.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TimeoutKind {
    Startup,
    TransportSilence,
    GracefulShutdown,
    ForcedTermination,
    ReceiveWait,
}

/// Deterministic process-configuration failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    EmptyExecutable,
    RelativeExecutable,
    RelativeWorkingDirectory,
    EmbeddedNul(ConfigField),
    ZeroTimeout(TimeoutKind),
    TimeoutTooLarge(TimeoutKind),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ConfigError {}

/// Stable Win32 operation label.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Win32Operation {
    CreatePipe,
    SetHandleInformation,
    CreateJob,
    ConfigureJob,
    CreateProcess,
    TerminateProcess,
    AssignProcessToJob,
    ResumeThread,
    ReadPipe,
    WritePipe,
    WaitForProcess,
    QueryExitCode,
    QueryJob,
    TerminateJob,
    CancelThreadIo,
    InitializeAttributeList,
    UpdateAttributeList,
}

/// A Win32 failure represented without localized diagnostic text.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PlatformFailure {
    operation: Win32Operation,
    code: u32,
}

impl PlatformFailure {
    pub(crate) const fn new(operation: Win32Operation, code: u32) -> Self {
        Self { operation, code }
    }

    /// Returns the failed operation.
    pub const fn operation(self) -> Win32Operation {
        self.operation
    }

    /// Returns the numeric `GetLastError` code.
    pub const fn code(self) -> u32 {
        self.code
    }
}

impl fmt::Display for PlatformFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} failed with Windows code {}",
            self.operation, self.code
        )
    }
}

impl Error for PlatformFailure {}

/// Worker I/O direction.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PipeStream {
    Stdin,
    Stdout,
    Stderr,
}

/// Owned supervisor thread.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ThreadKind {
    StdinWriter,
    StdoutReader,
    StderrReader,
}

/// Exact handshake failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HandshakeFailure {
    UnexpectedMessage,
    WrongRole,
}

/// Exact graceful-shutdown protocol failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ShutdownProtocolFailure {
    WrongStoppedRole,
    ExitedWithoutStopped,
    NonzeroExit(u32),
}

/// Deterministic cleanup failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CleanupFailure {
    JobDidNotEmpty,
    ThreadPanicked(ThreadKind),
}

/// A process/transport failure eligible for bounded restart.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RestartCause {
    StartupTimeout,
    TransportSilenceTimeout,
    UnexpectedEof,
    ProcessExited { code: u32 },
    PipeIo { stream: PipeStream, code: u32 },
    ThreadEnded { thread: ThreadKind },
}

/// Stable failure classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorFailureKind {
    Platform(PlatformFailure),
    Protocol(ProtocolError),
    Handshake(HandshakeFailure),
    ShutdownProtocol(ShutdownProtocolFailure),
    InvalidReceiveWait(ConfigError),
    InputBackpressure,
    OutputBackpressure,
    ThreadSpawn(ThreadKind),
    ThreadPanicked(ThreadKind),
    RestartExhausted {
        restarts_used: u8,
        last_cause: RestartCause,
    },
    Cleanup(CleanupFailure),
}

/// Supervisor failure with bounded diagnostic evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupervisorFailure {
    kind: SupervisorFailureKind,
    diagnostics: DiagnosticSnapshot,
    attempts_used: u16,
}

impl SupervisorFailure {
    pub(crate) const fn new(
        kind: SupervisorFailureKind,
        diagnostics: DiagnosticSnapshot,
        attempts_used: u16,
    ) -> Self {
        Self {
            kind,
            diagnostics,
            attempts_used,
        }
    }

    /// Returns the stable failure classification.
    pub const fn kind(&self) -> &SupervisorFailureKind {
        &self.kind
    }

    /// Returns bounded worker diagnostics captured before cleanup.
    pub const fn diagnostics(&self) -> &DiagnosticSnapshot {
        &self.diagnostics
    }

    /// Returns total launch attempts, including the initial attempt.
    pub const fn attempts_used(&self) -> u16 {
        self.attempts_used
    }
}

impl fmt::Display for SupervisorFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "worker supervision failed: {:?}", self.kind)
    }
}

impl Error for SupervisorFailure {}

impl From<PlatformFailure> for SupervisorFailureKind {
    fn from(value: PlatformFailure) -> Self {
        Self::Platform(value)
    }
}

impl From<ProtocolError> for SupervisorFailureKind {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}
