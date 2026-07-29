use crate::state::ShutdownState;
use crate::{
    ApplicationLifecycle, ApplicationState, AudioEpoch, DomainEvent, Effect, GenerationEpoch,
    GenerationState, RequestId, SessionId, SessionState, TurnId, TurnState, WorkerFault,
    WorkerKind,
};

/// The deterministic result of applying one domain event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    state: ApplicationState,
    effects: Vec<Effect>,
}

impl Transition {
    fn new(state: ApplicationState, effects: Vec<Effect>) -> Self {
        Self { state, effects }
    }

    /// Returns the authoritative state after the event was considered.
    pub fn state(&self) -> &ApplicationState {
        &self.state
    }

    /// Returns the ordered effects requested by the domain.
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    /// Consumes the transition and returns its state and ordered effects.
    pub fn into_parts(self) -> (ApplicationState, Vec<Effect>) {
        (self.state, self.effects)
    }
}

/// Applies one event to authoritative application state without performing I/O.
///
/// Ignored, stale, duplicate, mismatched, or otherwise invalid events return
/// the input state unchanged and emit no effects.
pub fn reduce(state: ApplicationState, event: DomainEvent) -> Transition {
    match state.lifecycle() {
        ApplicationLifecycle::Running => reduce_running(state, event),
        ApplicationLifecycle::ShuttingDown => reduce_shutting_down(state, event),
        ApplicationLifecycle::Stopped => unchanged(state),
    }
}

fn reduce_running(state: ApplicationState, event: DomainEvent) -> Transition {
    match event {
        DomainEvent::SessionOpened { session_id } => open_session(state, session_id),
        DomainEvent::SessionClosed { session_id } => close_session(state, session_id),
        DomainEvent::TurnOpened {
            session_id,
            turn_id,
        } => open_turn(state, session_id, turn_id),
        DomainEvent::TurnClosed {
            session_id,
            turn_id,
        } => close_turn(state, session_id, turn_id),
        DomainEvent::GenerationStarted {
            session_id,
            turn_id,
            generation_epoch,
            request_id,
        } => start_generation(state, session_id, turn_id, generation_epoch, request_id),
        DomainEvent::GenerationCompleted {
            session_id,
            turn_id,
            generation_epoch,
            request_id,
        }
        | DomainEvent::GenerationCancelled {
            session_id,
            turn_id,
            generation_epoch,
            request_id,
        } => finish_generation(state, session_id, turn_id, generation_epoch, request_id),
        DomainEvent::AudioEpochChanged {
            session_id,
            audio_epoch,
        } => change_audio_epoch(state, session_id, audio_epoch),
        DomainEvent::AudioStopped { audio_epoch } => stop_audio(state, audio_epoch),
        DomainEvent::WorkerFaulted { fault, .. } if fault != WorkerFault::Cancelled => {
            begin_shutdown(state)
        }
        DomainEvent::ShutdownRequested => begin_shutdown(state),
        DomainEvent::WorkerFaulted { .. }
        | DomainEvent::RequestCancellationCompleted { .. }
        | DomainEvent::WorkerStopped { .. }
        | DomainEvent::ShutdownCompleted => unchanged(state),
    }
}

fn reduce_shutting_down(state: ApplicationState, event: DomainEvent) -> Transition {
    match event {
        DomainEvent::RequestCancellationCompleted {
            generation_epoch,
            request_id,
        } => acknowledge_cancellation(state, generation_epoch, request_id),
        DomainEvent::AudioStopped { audio_epoch } => acknowledge_shutdown_audio(state, audio_epoch),
        DomainEvent::WorkerStopped { worker } => acknowledge_worker(state, worker),
        DomainEvent::ShutdownCompleted => complete_shutdown(state),
        DomainEvent::SessionOpened { .. }
        | DomainEvent::SessionClosed { .. }
        | DomainEvent::TurnOpened { .. }
        | DomainEvent::TurnClosed { .. }
        | DomainEvent::GenerationStarted { .. }
        | DomainEvent::GenerationCompleted { .. }
        | DomainEvent::GenerationCancelled { .. }
        | DomainEvent::AudioEpochChanged { .. }
        | DomainEvent::WorkerFaulted { .. }
        | DomainEvent::ShutdownRequested => unchanged(state),
    }
}

fn open_session(state: ApplicationState, session_id: SessionId) -> Transition {
    if state.session().is_some() {
        return unchanged(state);
    }

    transitioned(
        running(Some(SessionState::new(session_id, None, None))),
        Vec::new(),
    )
}

fn close_session(state: ApplicationState, session_id: SessionId) -> Transition {
    let Some(session) = state.session() else {
        return unchanged(state);
    };
    if session.session_id() != session_id {
        return unchanged(state);
    }

    let mut effects = Vec::new();
    if let Some(generation) = active_generation(Some(session)) {
        effects.push(cancel_effect(generation));
    }
    if let Some(audio_epoch) = session.audio_epoch() {
        effects.push(Effect::StopAudio {
            audio_epoch: Some(audio_epoch),
        });
    }

    transitioned(running(None), effects)
}

fn open_turn(state: ApplicationState, session_id: SessionId, turn_id: TurnId) -> Transition {
    let Some(session) = state.session() else {
        return unchanged(state);
    };
    if session.session_id() != session_id {
        return unchanged(state);
    }

    let cancellation = match session.active_turn() {
        None => None,
        Some(current) if turn_id > current.turn_id() => {
            current.active_generation().copied().map(cancel_effect)
        }
        Some(_) => return unchanged(state),
    };

    let (_, session, _) = state.into_parts();
    let session = session.expect("a matching running session was observed");
    let (stored_session_id, _, audio_epoch) = session.into_parts();
    let replacement = SessionState::new(
        stored_session_id,
        Some(TurnState::new(turn_id, None)),
        audio_epoch,
    );
    let effects = cancellation.into_iter().collect();

    transitioned(running(Some(replacement)), effects)
}

fn close_turn(state: ApplicationState, session_id: SessionId, turn_id: TurnId) -> Transition {
    let Some(session) = state.session() else {
        return unchanged(state);
    };
    if session.session_id() != session_id {
        return unchanged(state);
    }
    let Some(turn) = session.active_turn() else {
        return unchanged(state);
    };
    if turn.turn_id() != turn_id {
        return unchanged(state);
    }

    let effects = turn
        .active_generation()
        .copied()
        .map(cancel_effect)
        .into_iter()
        .collect();
    let (_, session, _) = state.into_parts();
    let session = session.expect("a matching running session was observed");
    let (stored_session_id, _, audio_epoch) = session.into_parts();

    transitioned(
        running(Some(SessionState::new(
            stored_session_id,
            None,
            audio_epoch,
        ))),
        effects,
    )
}

fn start_generation(
    state: ApplicationState,
    session_id: SessionId,
    turn_id: TurnId,
    generation_epoch: GenerationEpoch,
    request_id: RequestId,
) -> Transition {
    let Some(session) = state.session() else {
        return unchanged(state);
    };
    if session.session_id() != session_id {
        return unchanged(state);
    }
    let Some(turn) = session.active_turn() else {
        return unchanged(state);
    };
    if turn.turn_id() != turn_id {
        return unchanged(state);
    }

    let cancellation = match turn.active_generation().copied() {
        None => None,
        Some(current) if generation_epoch > current.generation_epoch() => {
            Some(cancel_effect(current))
        }
        Some(_) => return unchanged(state),
    };

    let (_, session, _) = state.into_parts();
    let session = session.expect("a matching running session was observed");
    let (stored_session_id, turn, audio_epoch) = session.into_parts();
    let turn = turn.expect("a matching active turn was observed");
    let (stored_turn_id, _) = turn.into_parts();
    let replacement = GenerationState::new(generation_epoch, request_id);
    let session = SessionState::new(
        stored_session_id,
        Some(TurnState::new(stored_turn_id, Some(replacement))),
        audio_epoch,
    );

    transitioned(running(Some(session)), cancellation.into_iter().collect())
}

fn finish_generation(
    state: ApplicationState,
    session_id: SessionId,
    turn_id: TurnId,
    generation_epoch: GenerationEpoch,
    request_id: RequestId,
) -> Transition {
    let Some(current) = active_generation(state.session()) else {
        return unchanged(state);
    };

    // GenerationEpoch is deliberately checked before every consistency field.
    if current.generation_epoch() != generation_epoch {
        return unchanged(state);
    }

    let session = state
        .session()
        .expect("an active generation always belongs to a session");
    let turn = session
        .active_turn()
        .expect("an active generation always belongs to a turn");
    if session.session_id() != session_id
        || turn.turn_id() != turn_id
        || current.request_id() != request_id
    {
        return unchanged(state);
    }

    let (_, session, _) = state.into_parts();
    let session = session.expect("a matching running session was observed");
    let (stored_session_id, turn, audio_epoch) = session.into_parts();
    let turn = turn.expect("a matching active turn was observed");
    let (stored_turn_id, _) = turn.into_parts();
    let session = SessionState::new(
        stored_session_id,
        Some(TurnState::new(stored_turn_id, None)),
        audio_epoch,
    );

    transitioned(running(Some(session)), Vec::new())
}

fn change_audio_epoch(
    state: ApplicationState,
    session_id: SessionId,
    audio_epoch: AudioEpoch,
) -> Transition {
    let Some(session) = state.session() else {
        return unchanged(state);
    };
    if session.session_id() != session_id {
        return unchanged(state);
    }

    let stop = match session.audio_epoch() {
        None => None,
        Some(current) if audio_epoch > current => Some(Effect::StopAudio {
            audio_epoch: Some(current),
        }),
        Some(_) => return unchanged(state),
    };

    let (_, session, _) = state.into_parts();
    let session = session.expect("a matching running session was observed");
    let (stored_session_id, active_turn, _) = session.into_parts();
    let session = SessionState::new(stored_session_id, active_turn, Some(audio_epoch));

    transitioned(running(Some(session)), stop.into_iter().collect())
}

fn stop_audio(state: ApplicationState, audio_epoch: Option<AudioEpoch>) -> Transition {
    let Some(session) = state.session() else {
        return unchanged(state);
    };
    let Some(current) = session.audio_epoch() else {
        return unchanged(state);
    };
    if audio_epoch != Some(current) {
        return unchanged(state);
    }

    let (_, session, _) = state.into_parts();
    let session = session.expect("an active audio epoch belongs to a session");
    let (session_id, active_turn, _) = session.into_parts();

    transitioned(
        running(Some(SessionState::new(session_id, active_turn, None))),
        Vec::new(),
    )
}

fn begin_shutdown(state: ApplicationState) -> Transition {
    let generation = active_generation(state.session());
    let audio_epoch = state.session().and_then(SessionState::audio_epoch);
    let pending_cancellation =
        generation.map(|value| (value.generation_epoch(), value.request_id()));
    let shutdown = ShutdownState::new(pending_cancellation, audio_epoch);
    let mut effects = Vec::new();

    if let Some(generation) = generation {
        effects.push(cancel_effect(generation));
    }
    effects.push(Effect::StopAudio { audio_epoch });
    for worker in [
        WorkerKind::Asr,
        WorkerKind::Llm,
        WorkerKind::Tts,
        WorkerKind::Audio,
    ] {
        effects.push(Effect::StopWorker { worker });
    }

    let (_, session, _) = state.into_parts();
    transitioned(shutting_down(session, shutdown), effects)
}

fn acknowledge_cancellation(
    state: ApplicationState,
    generation_epoch: GenerationEpoch,
    request_id: RequestId,
) -> Transition {
    let (_, session, shutdown) = state.into_parts();
    let mut shutdown = shutdown.expect("shutting-down state has bookkeeping");
    let accepted = shutdown.acknowledge_cancellation(generation_epoch, request_id);
    let session = if accepted {
        clear_generation(session, generation_epoch, request_id)
    } else {
        session
    };

    after_acknowledgement(session, shutdown, accepted)
}

fn acknowledge_shutdown_audio(
    state: ApplicationState,
    audio_epoch: Option<AudioEpoch>,
) -> Transition {
    let (_, session, shutdown) = state.into_parts();
    let mut shutdown = shutdown.expect("shutting-down state has bookkeeping");
    let accepted = shutdown.acknowledge_audio_stop(audio_epoch);
    let session = if accepted {
        clear_audio_epoch(session, audio_epoch)
    } else {
        session
    };

    after_acknowledgement(session, shutdown, accepted)
}

fn acknowledge_worker(state: ApplicationState, worker: WorkerKind) -> Transition {
    let (_, session, shutdown) = state.into_parts();
    let mut shutdown = shutdown.expect("shutting-down state has bookkeeping");
    let accepted = shutdown.acknowledge_worker_stop(worker);

    after_acknowledgement(session, shutdown, accepted)
}

fn after_acknowledgement(
    session: Option<SessionState>,
    mut shutdown: ShutdownState,
    accepted: bool,
) -> Transition {
    let effects = if accepted && shutdown.mark_complete_shutdown_emitted() {
        vec![Effect::CompleteShutdown]
    } else {
        Vec::new()
    };

    transitioned(shutting_down(session, shutdown), effects)
}

fn complete_shutdown(state: ApplicationState) -> Transition {
    let (_, session, shutdown) = state.into_parts();
    let shutdown = shutdown.expect("shutting-down state has bookkeeping");
    if !shutdown.acknowledgements_complete() || !shutdown.complete_shutdown_emitted() {
        return transitioned(shutting_down(session, shutdown), Vec::new());
    }

    transitioned(stopped(), Vec::new())
}

fn clear_generation(
    session: Option<SessionState>,
    generation_epoch: GenerationEpoch,
    request_id: RequestId,
) -> Option<SessionState> {
    session.map(|session| {
        let (session_id, active_turn, audio_epoch) = session.into_parts();
        let active_turn = active_turn.map(|turn| {
            let (turn_id, active_generation) = turn.into_parts();
            let active_generation = active_generation.filter(|generation| {
                generation.generation_epoch() != generation_epoch
                    || generation.request_id() != request_id
            });
            TurnState::new(turn_id, active_generation)
        });
        SessionState::new(session_id, active_turn, audio_epoch)
    })
}

fn clear_audio_epoch(
    session: Option<SessionState>,
    acknowledged_epoch: Option<AudioEpoch>,
) -> Option<SessionState> {
    session.map(|session| {
        let (session_id, active_turn, current_epoch) = session.into_parts();
        let audio_epoch = if current_epoch == acknowledged_epoch {
            None
        } else {
            current_epoch
        };
        SessionState::new(session_id, active_turn, audio_epoch)
    })
}

fn active_generation(session: Option<&SessionState>) -> Option<GenerationState> {
    session?.active_turn()?.active_generation().copied()
}

fn cancel_effect(generation: GenerationState) -> Effect {
    Effect::CancelRequest {
        generation_epoch: generation.generation_epoch(),
        request_id: generation.request_id(),
    }
}

fn running(session: Option<SessionState>) -> ApplicationState {
    ApplicationState::from_parts(ApplicationLifecycle::Running, session, None)
        .expect("running state does not carry shutdown bookkeeping")
}

fn shutting_down(session: Option<SessionState>, shutdown: ShutdownState) -> ApplicationState {
    ApplicationState::from_parts(ApplicationLifecycle::ShuttingDown, session, Some(shutdown))
        .expect("shutting-down state carries shutdown bookkeeping")
}

fn stopped() -> ApplicationState {
    ApplicationState::from_parts(ApplicationLifecycle::Stopped, None, None)
        .expect("stopped state has no session or shutdown bookkeeping")
}

fn transitioned(state: ApplicationState, effects: Vec<Effect>) -> Transition {
    Transition::new(state, effects)
}

fn unchanged(state: ApplicationState) -> Transition {
    transitioned(state, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::{Transition, reduce};
    use crate::{
        ApplicationLifecycle, ApplicationState, AudioEpoch, DomainEvent, Effect, GenerationEpoch,
        RequestId, SessionId, TurnId, WorkerFault, WorkerKind,
    };

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

    fn apply(state: ApplicationState, event: DomainEvent) -> ApplicationState {
        let transition = reduce(state, event);
        assert!(transition.effects().is_empty());
        transition.into_parts().0
    }

    fn session_state() -> ApplicationState {
        apply(
            ApplicationState::initial(),
            DomainEvent::SessionOpened {
                session_id: session_id(1),
            },
        )
    }

    fn turn_state() -> ApplicationState {
        apply(
            session_state(),
            DomainEvent::TurnOpened {
                session_id: session_id(1),
                turn_id: turn_id(2),
            },
        )
    }

    fn generation_state() -> ApplicationState {
        apply(
            turn_state(),
            DomainEvent::GenerationStarted {
                session_id: session_id(1),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
        )
    }

    fn active_state() -> ApplicationState {
        apply(
            generation_state(),
            DomainEvent::AudioEpochChanged {
                session_id: session_id(1),
                audio_epoch: audio_epoch(5),
            },
        )
    }

    fn assert_ignored(state: &ApplicationState, event: DomainEvent) {
        assert_eq!(
            reduce(state.clone(), event),
            Transition::new(state.clone(), Vec::new())
        );
    }

    #[test]
    fn transition_observation_and_consumption_preserve_ordered_output() {
        let transition = reduce(
            active_state(),
            DomainEvent::SessionClosed {
                session_id: session_id(1),
            },
        );
        let expected = vec![
            Effect::CancelRequest {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
            Effect::StopAudio {
                audio_epoch: Some(audio_epoch(5)),
            },
        ];

        assert_eq!(transition.state().session(), None);
        assert_eq!(transition.effects(), expected);
        let (state, effects) = transition.into_parts();
        assert_eq!(state.lifecycle(), ApplicationLifecycle::Running);
        assert_eq!(effects, expected);
    }

    #[test]
    fn equal_inputs_produce_equal_transitions() {
        let state = active_state();
        let event = DomainEvent::ShutdownRequested;

        assert_eq!(reduce(state.clone(), event.clone()), reduce(state, event));
    }

    #[test]
    fn sessions_open_once_and_only_matching_sessions_close() {
        let initial = ApplicationState::initial();
        assert_ignored(
            &session_state(),
            DomainEvent::SessionOpened {
                session_id: session_id(1),
            },
        );
        assert_ignored(
            &session_state(),
            DomainEvent::SessionOpened {
                session_id: session_id(9),
            },
        );
        assert_ignored(
            &initial,
            DomainEvent::SessionClosed {
                session_id: session_id(1),
            },
        );
        assert_ignored(
            &session_state(),
            DomainEvent::SessionClosed {
                session_id: session_id(9),
            },
        );

        let closed = reduce(
            active_state(),
            DomainEvent::SessionClosed {
                session_id: session_id(1),
            },
        );
        assert_eq!(closed.state().session(), None);
        assert_eq!(
            closed.effects(),
            [
                Effect::CancelRequest {
                    generation_epoch: generation_epoch(3),
                    request_id: request_id(4),
                },
                Effect::StopAudio {
                    audio_epoch: Some(audio_epoch(5)),
                },
            ]
        );
    }

    #[test]
    fn turn_ordering_is_session_local_and_replacement_cancels_generation() {
        let state = generation_state();
        for stale in [turn_id(1), turn_id(2)] {
            assert_ignored(
                &state,
                DomainEvent::TurnOpened {
                    session_id: session_id(1),
                    turn_id: stale,
                },
            );
        }
        assert_ignored(
            &state,
            DomainEvent::TurnOpened {
                session_id: session_id(9),
                turn_id: turn_id(8),
            },
        );

        let replacement = reduce(
            state,
            DomainEvent::TurnOpened {
                session_id: session_id(1),
                turn_id: turn_id(8),
            },
        );
        assert_eq!(
            replacement.effects(),
            [Effect::CancelRequest {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            }]
        );
        let turn = replacement
            .state()
            .session()
            .expect("session remains")
            .active_turn()
            .expect("replacement turn exists");
        assert_eq!(turn.turn_id(), turn_id(8));
        assert_eq!(turn.active_generation(), None);
    }

    #[test]
    fn turn_close_requires_exact_identity_and_replay_is_effect_free() {
        let state = active_state();
        for (session, turn) in [(9, 2), (1, 9)] {
            assert_ignored(
                &state,
                DomainEvent::TurnClosed {
                    session_id: session_id(session),
                    turn_id: turn_id(turn),
                },
            );
        }

        let closed = reduce(
            state,
            DomainEvent::TurnClosed {
                session_id: session_id(1),
                turn_id: turn_id(2),
            },
        );
        assert_eq!(
            closed.effects(),
            [Effect::CancelRequest {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            }]
        );
        assert_eq!(
            closed
                .state()
                .session()
                .expect("session remains")
                .active_turn(),
            None
        );
        assert_ignored(
            closed.state(),
            DomainEvent::TurnClosed {
                session_id: session_id(1),
                turn_id: turn_id(2),
            },
        );
    }

    #[test]
    fn generation_start_accepts_only_fresher_epochs_and_cancels_replaced_work() {
        let state = generation_state();
        for (epoch, request) in [(2, 8), (3, 4), (3, 9)] {
            assert_ignored(
                &state,
                DomainEvent::GenerationStarted {
                    session_id: session_id(1),
                    turn_id: turn_id(2),
                    generation_epoch: generation_epoch(epoch),
                    request_id: request_id(request),
                },
            );
        }

        let replacement = reduce(
            state,
            DomainEvent::GenerationStarted {
                session_id: session_id(1),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(5),
                request_id: request_id(6),
            },
        );
        assert_eq!(
            replacement.effects(),
            [Effect::CancelRequest {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            }]
        );
        let generation = replacement
            .state()
            .session()
            .expect("session remains")
            .active_turn()
            .expect("turn remains")
            .active_generation()
            .expect("replacement generation exists");
        assert_eq!(generation.generation_epoch(), generation_epoch(5));
        assert_eq!(generation.request_id(), request_id(6));
    }

    #[test]
    fn generation_results_require_epoch_first_then_all_consistency_fields() {
        let state = generation_state();
        for event in [
            DomainEvent::GenerationCompleted {
                session_id: session_id(1),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(2),
                request_id: request_id(4),
            },
            DomainEvent::GenerationCompleted {
                session_id: session_id(1),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(8),
                request_id: request_id(4),
            },
            DomainEvent::GenerationCompleted {
                session_id: session_id(9),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
            DomainEvent::GenerationCompleted {
                session_id: session_id(1),
                turn_id: turn_id(9),
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
            DomainEvent::GenerationCompleted {
                session_id: session_id(1),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(3),
                request_id: request_id(9),
            },
        ] {
            assert_ignored(&state, event);
        }

        for completed in [true, false] {
            let event = if completed {
                DomainEvent::GenerationCompleted {
                    session_id: session_id(1),
                    turn_id: turn_id(2),
                    generation_epoch: generation_epoch(3),
                    request_id: request_id(4),
                }
            } else {
                DomainEvent::GenerationCancelled {
                    session_id: session_id(1),
                    turn_id: turn_id(2),
                    generation_epoch: generation_epoch(3),
                    request_id: request_id(4),
                }
            };
            let finished = reduce(state.clone(), event.clone());
            assert_eq!(
                finished
                    .state()
                    .session()
                    .expect("session remains")
                    .active_turn()
                    .expect("turn remains")
                    .active_generation(),
                None
            );
            assert_ignored(finished.state(), event);
        }
    }

    #[test]
    fn audio_epoch_is_independent_and_freshness_first() {
        let state = active_state();
        for epoch in [4, 5] {
            assert_ignored(
                &state,
                DomainEvent::AudioEpochChanged {
                    session_id: session_id(1),
                    audio_epoch: audio_epoch(epoch),
                },
            );
        }
        assert_ignored(
            &state,
            DomainEvent::AudioEpochChanged {
                session_id: session_id(9),
                audio_epoch: audio_epoch(8),
            },
        );

        let changed = reduce(
            state,
            DomainEvent::AudioEpochChanged {
                session_id: session_id(1),
                audio_epoch: audio_epoch(8),
            },
        );
        assert_eq!(
            changed.effects(),
            [Effect::StopAudio {
                audio_epoch: Some(audio_epoch(5)),
            }]
        );
        let session = changed.state().session().expect("session remains");
        assert_eq!(session.audio_epoch(), Some(audio_epoch(8)));
        assert_eq!(
            session
                .active_turn()
                .expect("turn remains")
                .active_generation()
                .expect("generation remains")
                .generation_epoch(),
            generation_epoch(3)
        );
    }

    #[test]
    fn running_audio_stop_requires_an_exact_present_epoch() {
        let state = active_state();
        for observed in [None, Some(audio_epoch(4)), Some(audio_epoch(6))] {
            assert_ignored(
                &state,
                DomainEvent::AudioStopped {
                    audio_epoch: observed,
                },
            );
        }

        let stopped = apply(
            state,
            DomainEvent::AudioStopped {
                audio_epoch: Some(audio_epoch(5)),
            },
        );
        assert_eq!(
            stopped.session().expect("session remains").audio_epoch(),
            None
        );
    }

    #[test]
    fn running_acknowledgements_and_completion_are_ignored() {
        let state = active_state();
        for event in [
            DomainEvent::RequestCancellationCompleted {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
            DomainEvent::WorkerStopped {
                worker: WorkerKind::Asr,
            },
            DomainEvent::ShutdownCompleted,
        ] {
            assert_ignored(&state, event);
        }
    }

    #[test]
    fn shutdown_initiation_has_exact_effect_order_and_retains_state() {
        let transition = reduce(active_state(), DomainEvent::ShutdownRequested);

        assert_eq!(
            transition.effects(),
            [
                Effect::CancelRequest {
                    generation_epoch: generation_epoch(3),
                    request_id: request_id(4),
                },
                Effect::StopAudio {
                    audio_epoch: Some(audio_epoch(5)),
                },
                Effect::StopWorker {
                    worker: WorkerKind::Asr,
                },
                Effect::StopWorker {
                    worker: WorkerKind::Llm,
                },
                Effect::StopWorker {
                    worker: WorkerKind::Tts,
                },
                Effect::StopWorker {
                    worker: WorkerKind::Audio,
                },
            ]
        );
        assert_eq!(
            transition.state().lifecycle(),
            ApplicationLifecycle::ShuttingDown
        );
        assert!(transition.state().session().is_some());
    }

    #[test]
    fn shutdown_without_active_work_still_requires_audio_and_workers() {
        let transition = reduce(ApplicationState::initial(), DomainEvent::ShutdownRequested);
        assert_eq!(
            transition.effects(),
            [
                Effect::StopAudio { audio_epoch: None },
                Effect::StopWorker {
                    worker: WorkerKind::Asr,
                },
                Effect::StopWorker {
                    worker: WorkerKind::Llm,
                },
                Effect::StopWorker {
                    worker: WorkerKind::Tts,
                },
                Effect::StopWorker {
                    worker: WorkerKind::Audio,
                },
            ]
        );
    }

    #[test]
    fn fault_request_and_worker_never_select_or_freshen_generation() {
        let state = active_state();
        let expected = reduce(
            state.clone(),
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Asr,
                request_id: None,
                fault: WorkerFault::Protocol,
            },
        );
        for event in [
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Asr,
                request_id: Some(request_id(4)),
                fault: WorkerFault::Protocol,
            },
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Asr,
                request_id: Some(request_id(99)),
                fault: WorkerFault::Protocol,
            },
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Llm,
                request_id: Some(request_id(99)),
                fault: WorkerFault::Protocol,
            },
        ] {
            assert_eq!(reduce(state.clone(), event), expected);
        }
        assert_eq!(
            expected.effects().first(),
            Some(&Effect::CancelRequest {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            })
        );
    }

    #[test]
    fn cancelled_worker_fault_is_a_no_op_for_every_request_form() {
        let state = active_state();
        for request_id in [None, Some(request_id(4)), Some(request_id(99))] {
            assert_ignored(
                &state,
                DomainEvent::WorkerFaulted {
                    worker: WorkerKind::Tts,
                    request_id,
                    fault: WorkerFault::Cancelled,
                },
            );
        }
    }

    #[test]
    fn fatal_fault_classifications_share_the_shutdown_policy() {
        let state = active_state();
        let expected = reduce(
            state.clone(),
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Audio,
                request_id: None,
                fault: WorkerFault::Unavailable,
            },
        );
        for fault in [
            WorkerFault::Protocol,
            WorkerFault::InvalidResponse,
            WorkerFault::UnexpectedTermination,
        ] {
            assert_eq!(
                reduce(
                    state.clone(),
                    DomainEvent::WorkerFaulted {
                        worker: WorkerKind::Audio,
                        request_id: None,
                        fault,
                    }
                ),
                expected
            );
        }
    }

    #[test]
    fn shutting_down_ignores_non_acknowledgement_events() {
        let state = reduce(active_state(), DomainEvent::ShutdownRequested)
            .into_parts()
            .0;
        for event in [
            DomainEvent::SessionClosed {
                session_id: session_id(1),
            },
            DomainEvent::TurnOpened {
                session_id: session_id(1),
                turn_id: turn_id(9),
            },
            DomainEvent::GenerationStarted {
                session_id: session_id(1),
                turn_id: turn_id(2),
                generation_epoch: generation_epoch(9),
                request_id: request_id(9),
            },
            DomainEvent::AudioEpochChanged {
                session_id: session_id(1),
                audio_epoch: audio_epoch(9),
            },
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Llm,
                request_id: None,
                fault: WorkerFault::Protocol,
            },
            DomainEvent::ShutdownRequested,
        ] {
            assert_ignored(&state, event);
        }
    }

    #[test]
    fn shutdown_acknowledgements_are_exact_duplicate_safe_and_emit_completion_once() {
        let mut state = reduce(active_state(), DomainEvent::ShutdownRequested)
            .into_parts()
            .0;
        assert_ignored(&state, DomainEvent::ShutdownCompleted);
        for event in [
            DomainEvent::RequestCancellationCompleted {
                generation_epoch: generation_epoch(2),
                request_id: request_id(4),
            },
            DomainEvent::RequestCancellationCompleted {
                generation_epoch: generation_epoch(3),
                request_id: request_id(99),
            },
            DomainEvent::AudioStopped { audio_epoch: None },
            DomainEvent::AudioStopped {
                audio_epoch: Some(audio_epoch(4)),
            },
        ] {
            assert_ignored(&state, event);
        }

        state = apply(
            state,
            DomainEvent::RequestCancellationCompleted {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
        );
        assert_eq!(
            state
                .session()
                .expect("retained session")
                .active_turn()
                .expect("retained turn")
                .active_generation(),
            None
        );
        assert_ignored(
            &state,
            DomainEvent::RequestCancellationCompleted {
                generation_epoch: generation_epoch(3),
                request_id: request_id(4),
            },
        );

        state = apply(
            state,
            DomainEvent::AudioStopped {
                audio_epoch: Some(audio_epoch(5)),
            },
        );
        assert_eq!(
            state.session().expect("retained session").audio_epoch(),
            None
        );
        assert_ignored(
            &state,
            DomainEvent::AudioStopped {
                audio_epoch: Some(audio_epoch(5)),
            },
        );

        for worker in [WorkerKind::Asr, WorkerKind::Llm, WorkerKind::Tts] {
            state = apply(state, DomainEvent::WorkerStopped { worker });
            assert_ignored(&state, DomainEvent::WorkerStopped { worker });
        }
        let completion = reduce(
            state,
            DomainEvent::WorkerStopped {
                worker: WorkerKind::Audio,
            },
        );
        assert_eq!(completion.effects(), [Effect::CompleteShutdown]);
        state = completion.into_parts().0;

        assert_ignored(
            &state,
            DomainEvent::WorkerStopped {
                worker: WorkerKind::Audio,
            },
        );
        let completed = reduce(state, DomainEvent::ShutdownCompleted);
        assert!(completed.effects().is_empty());
        assert_eq!(completed.state().lifecycle(), ApplicationLifecycle::Stopped);
        assert_eq!(completed.state().session(), None);
        assert_ignored(completed.state(), DomainEvent::ShutdownCompleted);
    }

    #[test]
    fn absent_audio_target_requires_exact_none_acknowledgement() {
        let mut state = reduce(ApplicationState::initial(), DomainEvent::ShutdownRequested)
            .into_parts()
            .0;
        assert_ignored(
            &state,
            DomainEvent::AudioStopped {
                audio_epoch: Some(audio_epoch(1)),
            },
        );
        state = apply(state, DomainEvent::AudioStopped { audio_epoch: None });
        assert_ignored(&state, DomainEvent::AudioStopped { audio_epoch: None });
    }

    #[test]
    fn stopped_state_ignores_every_event() {
        let mut state = reduce(ApplicationState::initial(), DomainEvent::ShutdownRequested)
            .into_parts()
            .0;
        state = apply(state, DomainEvent::AudioStopped { audio_epoch: None });
        for worker in [WorkerKind::Asr, WorkerKind::Llm, WorkerKind::Tts] {
            state = apply(state, DomainEvent::WorkerStopped { worker });
        }
        let ready = reduce(
            state,
            DomainEvent::WorkerStopped {
                worker: WorkerKind::Audio,
            },
        );
        assert_eq!(ready.effects(), [Effect::CompleteShutdown]);
        let state = reduce(ready.into_parts().0, DomainEvent::ShutdownCompleted)
            .into_parts()
            .0;

        for event in [
            DomainEvent::SessionOpened {
                session_id: session_id(1),
            },
            DomainEvent::ShutdownRequested,
            DomainEvent::WorkerStopped {
                worker: WorkerKind::Asr,
            },
        ] {
            assert_ignored(&state, event);
        }
    }
}
