use crate::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId, WorkerKind};

/// Top-level lifecycle of the Hector application.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ApplicationLifecycle {
    /// The application accepts normal domain events.
    Running,
    /// Graceful shutdown has begun but may still retain active session state.
    ShuttingDown,
    /// Shutdown is complete and no session remains.
    Stopped,
}

/// Authoritative application state owned by the main Rust process.
///
/// The public API exposes observation only. In particular, external code
/// cannot assemble arbitrary lifecycle and session combinations.
///
/// ```compile_fail
/// use hector_core::{ApplicationLifecycle, ApplicationState};
///
/// let _ = ApplicationState {
///     lifecycle: ApplicationLifecycle::Running,
///     session: None,
///     shutdown: None,
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationState {
    lifecycle: ApplicationLifecycle,
    session: Option<SessionState>,
    shutdown: Option<ShutdownState>,
}

impl ApplicationState {
    /// Constructs the deterministic initial state.
    pub fn initial() -> Self {
        Self::from_parts(ApplicationLifecycle::Running, None, None)
            .expect("the initial application state is valid")
    }

    /// Returns the current application lifecycle.
    pub fn lifecycle(&self) -> ApplicationLifecycle {
        self.lifecycle
    }

    /// Returns the current session, if one exists.
    pub fn session(&self) -> Option<&SessionState> {
        self.session.as_ref()
    }

    pub(crate) fn from_parts(
        lifecycle: ApplicationLifecycle,
        session: Option<SessionState>,
        shutdown: Option<ShutdownState>,
    ) -> Option<Self> {
        let valid = match lifecycle {
            ApplicationLifecycle::Running => shutdown.is_none(),
            ApplicationLifecycle::ShuttingDown => shutdown.is_some(),
            ApplicationLifecycle::Stopped => session.is_none() && shutdown.is_none(),
        };

        if valid {
            Some(Self {
                lifecycle,
                session,
                shutdown,
            })
        } else {
            None
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        ApplicationLifecycle,
        Option<SessionState>,
        Option<ShutdownState>,
    ) {
        (self.lifecycle, self.session, self.shutdown)
    }
}

/// State belonging to one conversation session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionState {
    session_id: SessionId,
    active_turn: Option<TurnState>,
    audio_epoch: Option<AudioEpoch>,
}

impl SessionState {
    pub(crate) fn new(
        session_id: SessionId,
        active_turn: Option<TurnState>,
        audio_epoch: Option<AudioEpoch>,
    ) -> Self {
        Self {
            session_id,
            active_turn,
            audio_epoch,
        }
    }

    pub(crate) fn into_parts(self) -> (SessionId, Option<TurnState>, Option<AudioEpoch>) {
        (self.session_id, self.active_turn, self.audio_epoch)
    }

    /// Returns the identity of this session.
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the active turn, if one exists.
    pub fn active_turn(&self) -> Option<&TurnState> {
        self.active_turn.as_ref()
    }

    /// Returns the current audio-discontinuity epoch, if one exists.
    pub fn audio_epoch(&self) -> Option<AudioEpoch> {
        self.audio_epoch
    }
}

/// State belonging to one session-local turn.
///
/// A turn can enter application state only through crate-owned construction
/// beneath a [`SessionState`].
///
/// ```compile_fail
/// use hector_core::{TurnId, TurnState};
///
/// let turn_id = TurnId::from_raw(1).expect("one is nonzero");
/// let _ = TurnState::new(turn_id, None);
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnState {
    turn_id: TurnId,
    active_generation: Option<GenerationState>,
}

impl TurnState {
    pub(crate) fn new(turn_id: TurnId, active_generation: Option<GenerationState>) -> Self {
        Self {
            turn_id,
            active_generation,
        }
    }

    pub(crate) fn into_parts(self) -> (TurnId, Option<GenerationState>) {
        (self.turn_id, self.active_generation)
    }

    /// Returns this session-local turn's identity.
    pub fn turn_id(&self) -> TurnId {
        self.turn_id
    }

    /// Returns the active assistant generation, if one exists.
    pub fn active_generation(&self) -> Option<&GenerationState> {
        self.active_generation.as_ref()
    }
}

/// State identifying one active assistant generation.
///
/// The generation epoch and tracing request identity are always present
/// together. A generation can enter application state only beneath a
/// [`TurnState`].
///
/// ```compile_fail
/// use hector_core::{GenerationEpoch, GenerationState, RequestId};
///
/// let generation_epoch = GenerationEpoch::from_raw(1).expect("one is nonzero");
/// let request_id = RequestId::from_raw(1).expect("one is nonzero");
/// let _ = GenerationState::new(generation_epoch, request_id);
/// ```
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct GenerationState {
    generation_epoch: GenerationEpoch,
    request_id: RequestId,
}

impl GenerationState {
    pub(crate) fn new(generation_epoch: GenerationEpoch, request_id: RequestId) -> Self {
        Self {
            generation_epoch,
            request_id,
        }
    }

    /// Returns the sole assistant-output freshness fence for this generation.
    pub fn generation_epoch(self) -> GenerationEpoch {
        self.generation_epoch
    }

    /// Returns the tracing and correlation identity for this generation.
    pub fn request_id(self) -> RequestId {
        self.request_id
    }
}

/// Internal graceful-shutdown bookkeeping shared by state construction and the reducer.
///
/// This type is crate-private, has private fields, is not publicly re-exported,
/// and is used only as the internal boundary between `state.rs` and
/// `reducer.rs`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownState {
    pending_cancellation: Option<PendingCancellation>,
    pending_audio_stop: Option<PendingAudioStop>,
    pending_worker_stops: PendingWorkerStops,
    complete_shutdown_emitted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingCancellation {
    generation_epoch: GenerationEpoch,
    request_id: RequestId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingAudioStop {
    expected_audio_epoch: Option<AudioEpoch>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingWorkerStops {
    asr: bool,
    llm: bool,
    tts: bool,
    audio: bool,
}

impl ShutdownState {
    pub(crate) fn new(
        pending_cancellation: Option<(GenerationEpoch, RequestId)>,
        expected_audio_epoch: Option<AudioEpoch>,
    ) -> Self {
        Self {
            pending_cancellation: pending_cancellation.map(|(generation_epoch, request_id)| {
                PendingCancellation {
                    generation_epoch,
                    request_id,
                }
            }),
            pending_audio_stop: Some(PendingAudioStop {
                expected_audio_epoch,
            }),
            pending_worker_stops: PendingWorkerStops {
                asr: true,
                llm: true,
                tts: true,
                audio: true,
            },
            complete_shutdown_emitted: false,
        }
    }

    pub(crate) fn acknowledge_cancellation(
        &mut self,
        generation_epoch: GenerationEpoch,
        request_id: RequestId,
    ) -> bool {
        let matches = self.pending_cancellation.as_ref().is_some_and(|pending| {
            pending.generation_epoch == generation_epoch && pending.request_id == request_id
        });

        if matches {
            self.pending_cancellation = None;
        }

        matches
    }

    pub(crate) fn acknowledge_audio_stop(&mut self, audio_epoch: Option<AudioEpoch>) -> bool {
        let matches = self
            .pending_audio_stop
            .as_ref()
            .is_some_and(|pending| pending.expected_audio_epoch == audio_epoch);

        if matches {
            self.pending_audio_stop = None;
        }

        matches
    }

    pub(crate) fn acknowledge_worker_stop(&mut self, worker: WorkerKind) -> bool {
        let pending = match worker {
            WorkerKind::Asr => &mut self.pending_worker_stops.asr,
            WorkerKind::Llm => &mut self.pending_worker_stops.llm,
            WorkerKind::Tts => &mut self.pending_worker_stops.tts,
            WorkerKind::Audio => &mut self.pending_worker_stops.audio,
        };

        let was_pending = *pending;
        *pending = false;
        was_pending
    }

    pub(crate) fn acknowledgements_complete(&self) -> bool {
        self.pending_cancellation.is_none()
            && self.pending_audio_stop.is_none()
            && !self.pending_worker_stops.asr
            && !self.pending_worker_stops.llm
            && !self.pending_worker_stops.tts
            && !self.pending_worker_stops.audio
    }

    pub(crate) fn complete_shutdown_emitted(&self) -> bool {
        self.complete_shutdown_emitted
    }

    pub(crate) fn mark_complete_shutdown_emitted(&mut self) -> bool {
        if !self.acknowledgements_complete() || self.complete_shutdown_emitted {
            return false;
        }

        self.complete_shutdown_emitted = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplicationLifecycle, ApplicationState, GenerationState, SessionState, ShutdownState,
        TurnState,
    };
    use crate::WorkerKind;
    use crate::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId};

    fn session_id(raw: u128) -> SessionId {
        SessionId::from_raw(raw).expect("test session identifiers are nonzero")
    }

    fn request_id(raw: u128) -> RequestId {
        RequestId::from_raw(raw).expect("test request identifiers are nonzero")
    }

    fn turn_id(raw: u64) -> TurnId {
        TurnId::from_raw(raw).expect("test turn identifiers are nonzero")
    }

    fn generation_epoch(raw: u64) -> GenerationEpoch {
        GenerationEpoch::from_raw(raw).expect("test generation epochs are nonzero")
    }

    fn audio_epoch(raw: u64) -> AudioEpoch {
        AudioEpoch::from_raw(raw).expect("test audio epochs are nonzero")
    }

    fn nested_session() -> SessionState {
        let generation = GenerationState::new(generation_epoch(3), request_id(4));
        let turn = TurnState::new(turn_id(2), Some(generation));
        SessionState::new(session_id(1), Some(turn), Some(audio_epoch(5)))
    }

    #[test]
    fn initial_state_is_exact_and_deterministic() {
        let first = ApplicationState::initial();
        let second = ApplicationState::initial();

        assert_eq!(first, second);
        assert_eq!(first.lifecycle(), ApplicationLifecycle::Running);
        assert_eq!(first.session(), None);
        assert_eq!(
            format!("{first:?}"),
            "ApplicationState { lifecycle: Running, session: None, shutdown: None }"
        );
    }

    #[test]
    fn valid_nested_state_preserves_every_identifier() {
        let state = ApplicationState::from_parts(
            ApplicationLifecycle::Running,
            Some(nested_session()),
            None,
        )
        .expect("running may contain a session");

        let session = state.session().expect("the session is present");
        let turn = session.active_turn().expect("the turn is present");
        let generation = turn.active_generation().expect("the generation is present");

        assert_eq!(session.session_id(), session_id(1));
        assert_eq!(turn.turn_id(), turn_id(2));
        assert_eq!(generation.generation_epoch(), generation_epoch(3));
        assert_eq!(generation.request_id(), request_id(4));
        assert_eq!(session.audio_epoch(), Some(audio_epoch(5)));
    }

    #[test]
    fn session_may_have_neither_turn_nor_audio_epoch() {
        let session = SessionState::new(session_id(1), None, None);
        let state =
            ApplicationState::from_parts(ApplicationLifecycle::Running, Some(session), None)
                .expect("running may contain an inactive session");

        let session = state.session().expect("the session is present");
        assert_eq!(session.active_turn(), None);
        assert_eq!(session.audio_epoch(), None);
    }

    #[test]
    fn shutting_down_may_retain_nested_session_state() {
        let state = ApplicationState::from_parts(
            ApplicationLifecycle::ShuttingDown,
            Some(nested_session()),
            Some(ShutdownState::new(None, None)),
        )
        .expect("graceful shutdown may retain a session");

        assert_eq!(state.lifecycle(), ApplicationLifecycle::ShuttingDown);
        assert!(state.session().is_some());
    }

    #[test]
    fn stopped_state_contains_no_session() {
        let stopped = ApplicationState::from_parts(ApplicationLifecycle::Stopped, None, None)
            .expect("stopped without a session is valid");

        assert_eq!(stopped.lifecycle(), ApplicationLifecycle::Stopped);
        assert_eq!(stopped.session(), None);
    }

    #[test]
    fn stopped_state_with_a_session_is_rejected() {
        let invalid = ApplicationState::from_parts(
            ApplicationLifecycle::Stopped,
            Some(nested_session()),
            None,
        );

        assert_eq!(invalid, None);
    }

    #[test]
    fn lifecycle_and_shutdown_bookkeeping_combinations_are_enforced() {
        let shutdown = ShutdownState::new(None, None);

        assert_eq!(
            ApplicationState::from_parts(
                ApplicationLifecycle::Running,
                None,
                Some(shutdown.clone())
            ),
            None
        );
        assert_eq!(
            ApplicationState::from_parts(ApplicationLifecycle::ShuttingDown, None, None),
            None
        );
        assert_eq!(
            ApplicationState::from_parts(ApplicationLifecycle::Stopped, None, Some(shutdown)),
            None
        );
    }

    #[test]
    fn shutdown_acknowledgements_require_exact_outstanding_targets() {
        let generation = generation_epoch(3);
        let request = request_id(4);
        let audio = audio_epoch(5);
        let mut shutdown = ShutdownState::new(Some((generation, request)), Some(audio));

        assert!(!shutdown.acknowledge_cancellation(generation_epoch(2), request));
        assert!(!shutdown.acknowledge_cancellation(generation, request_id(6)));
        assert!(shutdown.acknowledge_cancellation(generation, request));
        assert!(!shutdown.acknowledge_cancellation(generation, request));

        assert!(!shutdown.acknowledge_audio_stop(None));
        assert!(!shutdown.acknowledge_audio_stop(Some(audio_epoch(4))));
        assert!(shutdown.acknowledge_audio_stop(Some(audio)));
        assert!(!shutdown.acknowledge_audio_stop(Some(audio)));
    }

    #[test]
    fn absent_audio_epoch_is_still_a_pending_exact_target() {
        let mut shutdown = ShutdownState::new(None, None);

        assert!(!shutdown.acknowledge_audio_stop(Some(audio_epoch(1))));
        assert!(shutdown.acknowledge_audio_stop(None));
        assert!(!shutdown.acknowledge_audio_stop(None));
    }

    #[test]
    fn worker_acknowledgements_clear_once_and_completion_marks_once() {
        let mut shutdown = ShutdownState::new(None, None);

        assert!(shutdown.acknowledge_audio_stop(None));
        assert!(!shutdown.acknowledgements_complete());
        assert!(!shutdown.mark_complete_shutdown_emitted());

        for worker in [
            WorkerKind::Asr,
            WorkerKind::Llm,
            WorkerKind::Tts,
            WorkerKind::Audio,
        ] {
            assert!(shutdown.acknowledge_worker_stop(worker));
            assert!(!shutdown.acknowledge_worker_stop(worker));
        }

        assert!(shutdown.acknowledgements_complete());
        assert!(!shutdown.complete_shutdown_emitted());
        assert!(shutdown.mark_complete_shutdown_emitted());
        assert!(shutdown.complete_shutdown_emitted());
        assert!(!shutdown.mark_complete_shutdown_emitted());
    }
}
