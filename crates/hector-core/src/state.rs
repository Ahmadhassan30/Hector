use crate::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId};

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
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationState {
    lifecycle: ApplicationLifecycle,
    session: Option<SessionState>,
}

impl ApplicationState {
    /// Constructs the deterministic initial state.
    pub fn initial() -> Self {
        Self::from_parts(ApplicationLifecycle::Running, None)
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
    ) -> Option<Self> {
        if lifecycle == ApplicationLifecycle::Stopped && session.is_some() {
            return None;
        }

        Some(Self { lifecycle, session })
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
    #[allow(
        dead_code,
        reason = "crate-private constructor is reserved for H12 reducer state construction"
    )]
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
    #[allow(
        dead_code,
        reason = "crate-private constructor is reserved for H12 reducer state construction"
    )]
    pub(crate) fn new(turn_id: TurnId, active_generation: Option<GenerationState>) -> Self {
        Self {
            turn_id,
            active_generation,
        }
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
    #[allow(
        dead_code,
        reason = "crate-private constructor is reserved for H12 reducer state construction"
    )]
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

#[cfg(test)]
mod tests {
    use super::{ApplicationLifecycle, ApplicationState, GenerationState, SessionState, TurnState};
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
            "ApplicationState { lifecycle: Running, session: None }"
        );
    }

    #[test]
    fn valid_nested_state_preserves_every_identifier() {
        let state =
            ApplicationState::from_parts(ApplicationLifecycle::Running, Some(nested_session()))
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
        let state = ApplicationState::from_parts(ApplicationLifecycle::Running, Some(session))
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
        )
        .expect("graceful shutdown may retain a session");

        assert_eq!(state.lifecycle(), ApplicationLifecycle::ShuttingDown);
        assert!(state.session().is_some());
    }

    #[test]
    fn stopped_state_contains_no_session() {
        let stopped = ApplicationState::from_parts(ApplicationLifecycle::Stopped, None)
            .expect("stopped without a session is valid");

        assert_eq!(stopped.lifecycle(), ApplicationLifecycle::Stopped);
        assert_eq!(stopped.session(), None);
    }

    #[test]
    fn stopped_state_with_a_session_is_rejected() {
        let invalid =
            ApplicationState::from_parts(ApplicationLifecycle::Stopped, Some(nested_session()));

        assert_eq!(invalid, None);
    }
}
