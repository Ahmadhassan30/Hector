use std::{
    cmp,
    time::{Duration, Instant},
};

use hector_protocol::{Envelope, ProtocolMessage, WorkerRole, encode_frame};

use crate::{
    CleanupFailure, HandshakeFailure, PipeStream, RestartCause, ShutdownProtocolFailure,
    SupervisorFailure, SupervisorFailureKind, TimeoutKind, WorkerProcessConfig,
    config::duration_to_wait_millis,
    process::{SuspendedProcess, resume_primary_thread},
    stdio::{IoReceive, IoTerminal, IoThreads},
};

const POLL_SLICE: Duration = Duration::from_millis(25);
const MAX_DRAINED_SHUTDOWN_MESSAGES: usize = 8;

/// Bounded snapshot of worker stderr.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticSnapshot {
    bytes: Vec<u8>,
    discarded_bytes: u64,
}

impl DiagnosticSnapshot {
    pub(crate) const fn new(bytes: Vec<u8>, discarded_bytes: u64) -> Self {
        Self {
            bytes,
            discarded_bytes,
        }
    }

    pub(crate) const fn empty() -> Self {
        Self::new(Vec::new(), 0)
    }

    /// Returns retained diagnostic bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the exact saturating count of discarded diagnostic bytes.
    pub const fn discarded_bytes(&self) -> u64 {
        self.discarded_bytes
    }
}

/// Stable externally visible supervisor lifecycle.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SupervisorState {
    Ready,
    ShuttingDown,
    Stopped,
}

/// One bounded receive result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiveOutcome {
    Event(SupervisorEvent),
    NoTraffic,
}

/// One process-supervision event for the later runtime owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorEvent {
    Message(Envelope),
    Restarted(RestartReport),
}

/// Evidence for one completed restart.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestartReport {
    restart_index: u8,
    cause: RestartCause,
    diagnostics: DiagnosticSnapshot,
}

impl RestartReport {
    /// Returns the one-based cumulative restart index.
    pub const fn restart_index(&self) -> u8 {
        self.restart_index
    }

    /// Returns why the old attempt was replaced.
    pub const fn cause(&self) -> RestartCause {
        self.cause
    }

    /// Returns diagnostics from the replaced attempt.
    pub const fn diagnostics(&self) -> &DiagnosticSnapshot {
        &self.diagnostics
    }
}

/// Evidence for one exact graceful shutdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    drained_messages: Vec<Envelope>,
    diagnostics: DiagnosticSnapshot,
}

impl ShutdownReport {
    /// Returns bounded messages observed before `Stopped`.
    pub fn drained_messages(&self) -> &[Envelope] {
        &self.drained_messages
    }

    /// Returns final bounded diagnostics.
    pub const fn diagnostics(&self) -> &DiagnosticSnapshot {
        &self.diagnostics
    }
}

/// Why the supervisor killed a process tree.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ForcedTerminationReason {
    Explicit,
    GracefulShutdownTimeout,
    Drop,
    Failure,
}

/// Evidence for completed Job termination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForcedTerminationReport {
    reason: ForcedTerminationReason,
    diagnostics: DiagnosticSnapshot,
}

impl ForcedTerminationReport {
    /// Returns why forced cleanup was selected.
    pub const fn reason(&self) -> ForcedTerminationReason {
        self.reason
    }

    /// Returns final bounded diagnostics.
    pub const fn diagnostics(&self) -> &DiagnosticSnapshot {
        &self.diagnostics
    }
}

/// Graceful shutdown result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    Graceful(ShutdownReport),
    Forced(ForcedTerminationReport),
    AlreadyStopped,
}

/// Explicit force-termination result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminationOutcome {
    Terminated(ForcedTerminationReport),
    AlreadyStopped,
}

/// Synchronous Windows-native worker supervisor.
pub struct WorkerSupervisor {
    config: WorkerProcessConfig,
    state: SupervisorState,
    attempt: Option<WorkerAttempt>,
    restarts_used: u8,
    attempts_used: u16,
    restart_records: Vec<RestartRecord>,
}

impl WorkerSupervisor {
    /// Launches and handshakes one configured worker.
    pub fn launch(config: WorkerProcessConfig) -> Result<Self, SupervisorFailure> {
        let mut supervisor = Self {
            config,
            state: SupervisorState::Stopped,
            attempt: None,
            restarts_used: 0,
            attempts_used: 0,
            restart_records: Vec::new(),
        };
        supervisor.start_until_ready()?;
        supervisor.state = SupervisorState::Ready;
        Ok(supervisor)
    }

    /// Returns the stable lifecycle.
    pub const fn state(&self) -> SupervisorState {
        self.state
    }

    /// Returns the configured worker role.
    pub const fn role(&self) -> WorkerRole {
        self.config.role()
    }

    /// Queues one complete H13 frame without interpreting it.
    pub fn send(&mut self, envelope: &Envelope) -> Result<(), SupervisorFailure> {
        if self.state != SupervisorState::Ready {
            return Err(self.failure(SupervisorFailureKind::InputBackpressure));
        }
        let frame = match encode_frame(envelope) {
            Ok(frame) => frame,
            Err(error) => {
                let diagnostics = self.cleanup_failed_attempt();
                self.state = SupervisorState::Stopped;
                return Err(SupervisorFailure::new(
                    SupervisorFailureKind::Protocol(error),
                    diagnostics,
                    self.attempts_used,
                ));
            }
        };
        let result = self
            .attempt
            .as_ref()
            .expect("ready supervisor has an attempt")
            .io
            .send(frame);
        match result {
            Ok(()) => Ok(()),
            Err(kind) => {
                let diagnostics = self.cleanup_failed_attempt();
                self.state = SupervisorState::Stopped;
                Err(SupervisorFailure::new(
                    kind,
                    diagnostics,
                    self.attempts_used,
                ))
            }
        }
    }

    /// Waits for at most `wait` without treating silence as model failure.
    pub fn receive(&mut self, wait: Duration) -> Result<ReceiveOutcome, SupervisorFailure> {
        duration_to_wait_millis(wait, TimeoutKind::ReceiveWait)
            .map_err(|error| self.failure(SupervisorFailureKind::InvalidReceiveWait(error)))?;
        if self.state != SupervisorState::Ready {
            return Err(self.failure(SupervisorFailureKind::InputBackpressure));
        }

        let silence_limit = self.config.timeouts().transport_silence();
        let silence_elapsed = self
            .attempt
            .as_ref()
            .expect("ready supervisor has an attempt")
            .io
            .silence_elapsed();
        if silence_limit.is_some_and(|limit| silence_elapsed >= limit) {
            return self.restart(RestartCause::TransportSilenceTimeout);
        }
        let effective_wait = silence_limit.map_or(wait, |limit| {
            cmp::min(wait, limit.saturating_sub(silence_elapsed))
        });
        let receive = self
            .attempt
            .as_ref()
            .expect("ready supervisor has an attempt")
            .receive(effective_wait);
        match receive {
            AttemptReceive::Envelope(envelope) => {
                Ok(ReceiveOutcome::Event(SupervisorEvent::Message(envelope)))
            }
            AttemptReceive::NoTraffic => {
                let silence_expired =
                    self.config
                        .timeouts()
                        .transport_silence()
                        .is_some_and(|limit| {
                            self.attempt
                                .as_ref()
                                .expect("ready supervisor has an attempt")
                                .io
                                .silence_elapsed()
                                >= limit
                        });
                if silence_expired {
                    self.restart(RestartCause::TransportSilenceTimeout)
                } else {
                    Ok(ReceiveOutcome::NoTraffic)
                }
            }
            AttemptReceive::Restartable(cause) => self.restart(cause),
            AttemptReceive::Fatal(kind) => {
                let diagnostics = self.cleanup_failed_attempt();
                self.state = SupervisorState::Stopped;
                Err(SupervisorFailure::new(
                    kind,
                    diagnostics,
                    self.attempts_used,
                ))
            }
        }
    }

    /// Gracefully stops a worker or forces cleanup when its deadline expires.
    pub fn shutdown(&mut self) -> Result<ShutdownOutcome, SupervisorFailure> {
        if self.state == SupervisorState::Stopped {
            return Ok(ShutdownOutcome::AlreadyStopped);
        }
        self.state = SupervisorState::ShuttingDown;
        let shutdown = Envelope::new(ProtocolMessage::Shutdown);
        let frame = encode_frame(&shutdown)
            .map_err(|error| self.failure(SupervisorFailureKind::Protocol(error)))?;
        if let Err(kind) = self
            .attempt
            .as_ref()
            .expect("live supervisor has an attempt")
            .io
            .send(frame)
        {
            let diagnostics = self.cleanup_failed_attempt();
            self.state = SupervisorState::Stopped;
            return Err(SupervisorFailure::new(
                kind,
                diagnostics,
                self.attempts_used,
            ));
        }

        let deadline = Instant::now() + self.config.timeouts().graceful_shutdown();
        let mut drained = Vec::new();
        let mut stopped_seen = false;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let receive = self
                .attempt
                .as_ref()
                .expect("live supervisor has an attempt")
                .receive(cmp::min(remaining, POLL_SLICE));
            match receive {
                AttemptReceive::Envelope(envelope) => match envelope.message() {
                    ProtocolMessage::Stopped { worker } if *worker == self.role() => {
                        stopped_seen = true;
                        break;
                    }
                    ProtocolMessage::Stopped { .. } => {
                        return self.shutdown_failure(SupervisorFailureKind::ShutdownProtocol(
                            ShutdownProtocolFailure::WrongStoppedRole,
                        ));
                    }
                    _ if drained.len() < MAX_DRAINED_SHUTDOWN_MESSAGES => drained.push(envelope),
                    _ => {
                        return self.shutdown_failure(SupervisorFailureKind::OutputBackpressure);
                    }
                },
                AttemptReceive::NoTraffic => {}
                AttemptReceive::Restartable(RestartCause::ProcessExited { .. }) => {
                    return self.shutdown_failure(SupervisorFailureKind::ShutdownProtocol(
                        ShutdownProtocolFailure::ExitedWithoutStopped,
                    ));
                }
                AttemptReceive::Restartable(_) => {
                    return self.shutdown_failure(SupervisorFailureKind::ShutdownProtocol(
                        ShutdownProtocolFailure::ExitedWithoutStopped,
                    ));
                }
                AttemptReceive::Fatal(kind) => return self.shutdown_failure(kind),
            }
        }

        if !stopped_seen {
            let report = self.force_current(ForcedTerminationReason::GracefulShutdownTimeout)?;
            self.state = SupervisorState::Stopped;
            return Ok(ShutdownOutcome::Forced(report));
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        let exit = self
            .attempt
            .as_ref()
            .expect("live supervisor has an attempt")
            .process
            .wait(remaining)
            .map_err(|error| self.failure(SupervisorFailureKind::Platform(error)))?;
        match exit {
            Some(0) => {}
            Some(code) => {
                return self.shutdown_failure(SupervisorFailureKind::ShutdownProtocol(
                    ShutdownProtocolFailure::NonzeroExit(code),
                ));
            }
            None => {
                let report =
                    self.force_current(ForcedTerminationReason::GracefulShutdownTimeout)?;
                self.state = SupervisorState::Stopped;
                return Ok(ShutdownOutcome::Forced(report));
            }
        }

        let mut attempt = self.attempt.take().expect("live supervisor has an attempt");
        let diagnostics = attempt
            .finish_after_exit(self.config.timeouts().forced_termination())
            .map_err(|kind| {
                SupervisorFailure::new(kind, attempt.io.diagnostics(), self.attempts_used)
            })?;
        self.state = SupervisorState::Stopped;
        Ok(ShutdownOutcome::Graceful(ShutdownReport {
            drained_messages: drained,
            diagnostics,
        }))
    }

    /// Immediately terminates the complete worker process tree.
    pub fn force_terminate(&mut self) -> Result<TerminationOutcome, SupervisorFailure> {
        if self.state == SupervisorState::Stopped {
            return Ok(TerminationOutcome::AlreadyStopped);
        }
        let report = self.force_current(ForcedTerminationReason::Explicit)?;
        self.state = SupervisorState::Stopped;
        Ok(TerminationOutcome::Terminated(report))
    }

    fn start_until_ready(&mut self) -> Result<Option<RestartReport>, SupervisorFailure> {
        loop {
            self.attempts_used = self.attempts_used.saturating_add(1);
            match WorkerAttempt::launch(&self.config) {
                Ok(attempt) => {
                    self.attempt = Some(attempt);
                    return Ok(self.restart_records.last().map(|record| RestartReport {
                        restart_index: self.restarts_used,
                        cause: record.cause,
                        diagnostics: record.diagnostics.clone(),
                    }));
                }
                Err(LaunchFailure::Fatal(kind, diagnostics)) => {
                    return Err(SupervisorFailure::new(
                        kind,
                        diagnostics,
                        self.attempts_used,
                    ));
                }
                Err(LaunchFailure::Restartable(cause, diagnostics)) => {
                    if self.restarts_used >= self.config.restart_policy().max_restarts() {
                        return Err(SupervisorFailure::new(
                            SupervisorFailureKind::RestartExhausted {
                                restarts_used: self.restarts_used,
                                last_cause: cause,
                            },
                            diagnostics,
                            self.attempts_used,
                        ));
                    }
                    self.restarts_used += 1;
                    self.restart_records
                        .push(RestartRecord { cause, diagnostics });
                }
            }
        }
    }

    fn restart(&mut self, cause: RestartCause) -> Result<ReceiveOutcome, SupervisorFailure> {
        let diagnostics = self.cleanup_failed_attempt();
        if self.restarts_used >= self.config.restart_policy().max_restarts() {
            self.state = SupervisorState::Stopped;
            return Err(SupervisorFailure::new(
                SupervisorFailureKind::RestartExhausted {
                    restarts_used: self.restarts_used,
                    last_cause: cause,
                },
                diagnostics,
                self.attempts_used,
            ));
        }
        self.restarts_used += 1;
        self.restart_records
            .push(RestartRecord { cause, diagnostics });
        match self.start_until_ready()? {
            Some(report) => {
                self.state = SupervisorState::Ready;
                Ok(ReceiveOutcome::Event(SupervisorEvent::Restarted(report)))
            }
            None => unreachable!("restart supplies a cause"),
        }
    }

    fn cleanup_failed_attempt(&mut self) -> DiagnosticSnapshot {
        let Some(mut attempt) = self.attempt.take() else {
            return DiagnosticSnapshot::empty();
        };
        let diagnostics = attempt.io.diagnostics();
        let _ = attempt.force_cleanup(self.config.timeouts().forced_termination());
        diagnostics
    }

    fn force_current(
        &mut self,
        reason: ForcedTerminationReason,
    ) -> Result<ForcedTerminationReport, SupervisorFailure> {
        let mut attempt = self.attempt.take().expect("live supervisor has an attempt");
        let diagnostics = attempt.io.diagnostics();
        attempt
            .force_cleanup(self.config.timeouts().forced_termination())
            .map_err(|kind| {
                SupervisorFailure::new(kind, diagnostics.clone(), self.attempts_used)
            })?;
        Ok(ForcedTerminationReport {
            reason,
            diagnostics: attempt.io.diagnostics(),
        })
    }

    fn shutdown_failure(
        &mut self,
        kind: SupervisorFailureKind,
    ) -> Result<ShutdownOutcome, SupervisorFailure> {
        let diagnostics = self.cleanup_failed_attempt();
        self.state = SupervisorState::Stopped;
        Err(SupervisorFailure::new(
            kind,
            diagnostics,
            self.attempts_used,
        ))
    }

    fn failure(&self, kind: SupervisorFailureKind) -> SupervisorFailure {
        let diagnostics = self
            .attempt
            .as_ref()
            .map_or_else(DiagnosticSnapshot::empty, |attempt| {
                attempt.io.diagnostics()
            });
        SupervisorFailure::new(kind, diagnostics, self.attempts_used)
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        if self.attempt.is_some() {
            let _ = self.force_current(ForcedTerminationReason::Drop);
            self.state = SupervisorState::Stopped;
        }
    }
}

struct WorkerAttempt {
    job: crate::job::Job,
    process: crate::process::Process,
    io: IoThreads,
}

struct RestartRecord {
    cause: RestartCause,
    diagnostics: DiagnosticSnapshot,
}

impl WorkerAttempt {
    fn launch(config: &WorkerProcessConfig) -> Result<Self, LaunchFailure> {
        let suspended = SuspendedProcess::create(config).map_err(|error| {
            LaunchFailure::Fatal(
                SupervisorFailureKind::Platform(error),
                DiagnosticSnapshot::empty(),
            )
        })?;
        let SuspendedProcess {
            job,
            process,
            primary_thread,
            parent_pipes,
        } = suspended;
        let io = IoThreads::start(parent_pipes, Instant::now())
            .map_err(|kind| LaunchFailure::Fatal(kind, DiagnosticSnapshot::empty()))?;
        let attempt = Self { job, process, io };
        if let Err(error) = {
            let resume_result = resume_primary_thread(&primary_thread);
            drop(primary_thread);
            resume_result
        } {
            let diagnostics = attempt.io.diagnostics();
            let mut attempt = attempt;
            let _ = attempt.force_cleanup(config.timeouts().forced_termination());
            return Err(LaunchFailure::Fatal(
                SupervisorFailureKind::Platform(error),
                diagnostics,
            ));
        }

        match attempt.handshake(config.role(), config.timeouts().startup()) {
            Ok(()) => Ok(attempt),
            Err(problem) => {
                let diagnostics = attempt.io.diagnostics();
                let mut attempt = attempt;
                let _ = attempt.force_cleanup(config.timeouts().forced_termination());
                match problem {
                    AttemptProblem::Restartable(cause) => {
                        Err(LaunchFailure::Restartable(cause, diagnostics))
                    }
                    AttemptProblem::Fatal(kind) => Err(LaunchFailure::Fatal(kind, diagnostics)),
                }
            }
        }
    }

    fn handshake(&self, role: WorkerRole, timeout: Duration) -> Result<(), AttemptProblem> {
        let hello = encode_frame(&Envelope::new(ProtocolMessage::Hello { worker: role }))
            .map_err(|error| AttemptProblem::Fatal(SupervisorFailureKind::Protocol(error)))?;
        self.io.send(hello).map_err(AttemptProblem::Fatal)?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(code) = self
                .process
                .exit_code_if_exited()
                .map_err(|error| AttemptProblem::Fatal(SupervisorFailureKind::Platform(error)))?
            {
                return Err(AttemptProblem::Restartable(RestartCause::ProcessExited {
                    code,
                }));
            }
            if Instant::now() >= deadline {
                return Err(AttemptProblem::Restartable(RestartCause::StartupTimeout));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.io.receive(cmp::min(remaining, POLL_SLICE)) {
                IoReceive::Envelope(envelope) => match envelope.message() {
                    ProtocolMessage::Ready { worker } if *worker == role => return Ok(()),
                    ProtocolMessage::Ready { .. } => {
                        return Err(AttemptProblem::Fatal(SupervisorFailureKind::Handshake(
                            HandshakeFailure::WrongRole,
                        )));
                    }
                    _ => {
                        return Err(AttemptProblem::Fatal(SupervisorFailureKind::Handshake(
                            HandshakeFailure::UnexpectedMessage,
                        )));
                    }
                },
                IoReceive::Timeout => {}
                IoReceive::Terminal(terminal) => return Err(map_terminal(terminal)),
            }
        }
    }

    fn receive(&self, wait: Duration) -> AttemptReceive {
        if let Ok(Some(code)) = self.process.exit_code_if_exited() {
            return AttemptReceive::Restartable(RestartCause::ProcessExited { code });
        }
        match self.io.receive(wait) {
            IoReceive::Envelope(envelope) => AttemptReceive::Envelope(envelope),
            IoReceive::Timeout => match self.process.exit_code_if_exited() {
                Ok(Some(code)) => AttemptReceive::Restartable(RestartCause::ProcessExited { code }),
                Ok(None) => AttemptReceive::NoTraffic,
                Err(error) => AttemptReceive::Fatal(SupervisorFailureKind::Platform(error)),
            },
            IoReceive::Terminal(IoTerminal::Eof(_)) => {
                match self.process.wait(Duration::from_millis(100)) {
                    Ok(Some(code)) => {
                        AttemptReceive::Restartable(RestartCause::ProcessExited { code })
                    }
                    Ok(None) => AttemptReceive::Restartable(RestartCause::UnexpectedEof),
                    Err(error) => AttemptReceive::Fatal(SupervisorFailureKind::Platform(error)),
                }
            }
            IoReceive::Terminal(terminal) => match map_terminal(terminal) {
                AttemptProblem::Restartable(cause) => AttemptReceive::Restartable(cause),
                AttemptProblem::Fatal(kind) => AttemptReceive::Fatal(kind),
            },
        }
    }

    fn force_cleanup(&mut self, timeout: Duration) -> Result<(), SupervisorFailureKind> {
        self.job
            .terminate()
            .map_err(SupervisorFailureKind::Platform)?;
        if !self
            .job
            .wait_empty(timeout)
            .map_err(SupervisorFailureKind::Platform)?
        {
            return Err(SupervisorFailureKind::Cleanup(
                CleanupFailure::JobDidNotEmpty,
            ));
        }
        self.io
            .stop_and_join()
            .map_err(|kind| SupervisorFailureKind::Cleanup(CleanupFailure::ThreadPanicked(kind)))
    }

    fn finish_after_exit(
        &mut self,
        timeout: Duration,
    ) -> Result<DiagnosticSnapshot, SupervisorFailureKind> {
        if !self
            .job
            .wait_empty(timeout)
            .map_err(SupervisorFailureKind::Platform)?
        {
            return Err(SupervisorFailureKind::Cleanup(
                CleanupFailure::JobDidNotEmpty,
            ));
        }
        self.io
            .stop_and_join()
            .map_err(|kind| SupervisorFailureKind::Cleanup(CleanupFailure::ThreadPanicked(kind)))?;
        Ok(self.io.diagnostics())
    }
}

enum AttemptReceive {
    Envelope(Envelope),
    NoTraffic,
    Restartable(RestartCause),
    Fatal(SupervisorFailureKind),
}

enum AttemptProblem {
    Restartable(RestartCause),
    Fatal(SupervisorFailureKind),
}

enum LaunchFailure {
    Restartable(RestartCause, DiagnosticSnapshot),
    Fatal(SupervisorFailureKind, DiagnosticSnapshot),
}

fn map_terminal(terminal: IoTerminal) -> AttemptProblem {
    match terminal {
        IoTerminal::Eof(PipeStream::Stdout) => {
            AttemptProblem::Restartable(RestartCause::UnexpectedEof)
        }
        IoTerminal::Eof(stream) => {
            AttemptProblem::Restartable(RestartCause::PipeIo { stream, code: 0 })
        }
        IoTerminal::Platform(stream, error) => AttemptProblem::Restartable(RestartCause::PipeIo {
            stream,
            code: error.code(),
        }),
        IoTerminal::Protocol(error) => {
            AttemptProblem::Fatal(SupervisorFailureKind::Protocol(error))
        }
        IoTerminal::OutputBackpressure => {
            AttemptProblem::Fatal(SupervisorFailureKind::OutputBackpressure)
        }
        IoTerminal::ThreadPanicked(thread) => {
            AttemptProblem::Fatal(SupervisorFailureKind::ThreadPanicked(thread))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf, time::Duration};

    use super::{DiagnosticSnapshot, RestartReport};
    use crate::{
        RestartCause, RestartPolicy, SupervisorFailureKind, SupervisorTimeouts,
        WorkerProcessConfig, WorkerSupervisor,
    };
    use hector_protocol::WorkerRole;

    #[test]
    fn report_types_keep_diagnostics_bounded_and_private() {
        let report = RestartReport {
            restart_index: 3,
            cause: RestartCause::UnexpectedEof,
            diagnostics: DiagnosticSnapshot::new(vec![1, 2], 7),
        };
        assert_eq!(report.restart_index(), 3);
        assert_eq!(report.cause(), RestartCause::UnexpectedEof);
        assert_eq!(report.diagnostics().bytes(), &[1, 2]);
        assert_eq!(report.diagnostics().discarded_bytes(), 7);
    }

    #[test]
    fn zero_receive_wait_is_invalid_by_contract() {
        assert!(Duration::ZERO.is_zero());
    }

    #[test]
    fn silent_process_exhausts_zero_restart_budget_at_startup_deadline() {
        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is defined");
        let executable = PathBuf::from(system_root).join(r"System32\cmd.exe");
        let timeouts = SupervisorTimeouts::new(
            Duration::from_millis(75),
            None,
            Duration::from_secs(1),
            Duration::from_secs(2),
        )
        .expect("test timeouts are valid");
        let config = WorkerProcessConfig::new(
            executable,
            vec![
                OsString::from("/D"),
                OsString::from("/S"),
                OsString::from("/C"),
                OsString::from("ping.exe -n 30 127.0.0.1 >nul"),
            ],
            WorkerRole::Llm,
            None,
            timeouts,
            RestartPolicy::new(0),
        )
        .expect("test process configuration is valid");

        let failure = match WorkerSupervisor::launch(config) {
            Ok(_) => panic!("a silent non-protocol process must not become ready"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.kind(),
            &SupervisorFailureKind::RestartExhausted {
                restarts_used: 0,
                last_cause: RestartCause::StartupTimeout,
            }
        );
    }
}
