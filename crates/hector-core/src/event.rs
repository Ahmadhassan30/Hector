use crate::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId, WorkerFault, WorkerKind};

/// Observation or completed outcome presented to the pure domain reducer.
///
/// This enum defines control-plane vocabulary only. H11 assigns no transition,
/// precedence, freshness, cancellation, fault-recovery, or shutdown behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainEvent {
    /// A conversation session became available.
    SessionOpened { session_id: SessionId },
    /// A conversation session closed.
    SessionClosed { session_id: SessionId },
    /// A session-local turn began.
    TurnOpened {
        session_id: SessionId,
        turn_id: TurnId,
    },
    /// A session-local turn ended.
    TurnClosed {
        session_id: SessionId,
        turn_id: TurnId,
    },
    /// An assistant generation began.
    GenerationStarted {
        session_id: SessionId,
        turn_id: TurnId,
        generation_epoch: GenerationEpoch,
        request_id: RequestId,
    },
    /// An assistant generation completed.
    GenerationCompleted {
        session_id: SessionId,
        turn_id: TurnId,
        generation_epoch: GenerationEpoch,
        request_id: RequestId,
    },
    /// An assistant generation was cancelled.
    GenerationCancelled {
        session_id: SessionId,
        turn_id: TurnId,
        generation_epoch: GenerationEpoch,
        request_id: RequestId,
    },
    /// Audio continuity moved to a new epoch.
    AudioEpochChanged {
        session_id: SessionId,
        audio_epoch: AudioEpoch,
    },
    /// A worker reported a classified fault.
    WorkerFaulted {
        worker: WorkerKind,
        request_id: Option<RequestId>,
        fault: WorkerFault,
    },
    /// Cancellation of a request completed.
    RequestCancellationCompleted {
        generation_epoch: GenerationEpoch,
        request_id: RequestId,
    },
    /// A worker stopped.
    WorkerStopped { worker: WorkerKind },
    /// Audio stopped at the observed epoch, if one existed.
    AudioStopped { audio_epoch: Option<AudioEpoch> },
    /// Graceful application shutdown was requested.
    ShutdownRequested,
    /// Graceful application shutdown completed.
    ShutdownCompleted,
}

#[cfg(test)]
mod tests {
    use super::DomainEvent;
    use crate::{
        AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId, WorkerFault, WorkerKind,
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

    #[test]
    fn session_and_turn_events_preserve_identifiers() {
        let session = session_id(1);
        let turn = turn_id(2);

        for event in [
            DomainEvent::SessionOpened {
                session_id: session,
            },
            DomainEvent::SessionClosed {
                session_id: session,
            },
        ] {
            match event {
                DomainEvent::SessionOpened { session_id }
                | DomainEvent::SessionClosed { session_id } => {
                    assert_eq!(session_id, session);
                }
                _ => unreachable!("the test constructs only session events"),
            }
        }

        for event in [
            DomainEvent::TurnOpened {
                session_id: session,
                turn_id: turn,
            },
            DomainEvent::TurnClosed {
                session_id: session,
                turn_id: turn,
            },
        ] {
            match event {
                DomainEvent::TurnOpened {
                    session_id,
                    turn_id,
                }
                | DomainEvent::TurnClosed {
                    session_id,
                    turn_id,
                } => {
                    assert_eq!(session_id, session);
                    assert_eq!(turn_id, turn);
                }
                _ => unreachable!("the test constructs only turn events"),
            }
        }
    }

    #[test]
    fn generation_events_preserve_every_identifier() {
        let session = session_id(1);
        let turn = turn_id(2);
        let generation = generation_epoch(3);
        let request = request_id(4);

        for event in [
            DomainEvent::GenerationStarted {
                session_id: session,
                turn_id: turn,
                generation_epoch: generation,
                request_id: request,
            },
            DomainEvent::GenerationCompleted {
                session_id: session,
                turn_id: turn,
                generation_epoch: generation,
                request_id: request,
            },
            DomainEvent::GenerationCancelled {
                session_id: session,
                turn_id: turn,
                generation_epoch: generation,
                request_id: request,
            },
        ] {
            match event {
                DomainEvent::GenerationStarted {
                    session_id,
                    turn_id,
                    generation_epoch,
                    request_id,
                }
                | DomainEvent::GenerationCompleted {
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
                } => {
                    assert_eq!(session_id, session);
                    assert_eq!(turn_id, turn);
                    assert_eq!(generation_epoch, generation);
                    assert_eq!(request_id, request);
                }
                _ => unreachable!("the test constructs only generation events"),
            }
        }
    }

    #[test]
    fn audio_worker_and_completion_events_preserve_payloads() {
        let session = session_id(1);
        let request = request_id(2);
        let audio = audio_epoch(3);

        let audio_changed = DomainEvent::AudioEpochChanged {
            session_id: session,
            audio_epoch: audio,
        };
        match audio_changed {
            DomainEvent::AudioEpochChanged {
                session_id,
                audio_epoch,
            } => {
                assert_eq!(session_id, session);
                assert_eq!(audio_epoch, audio);
            }
            _ => unreachable!("the test constructs an audio-epoch event"),
        }

        let faulted = DomainEvent::WorkerFaulted {
            worker: WorkerKind::Llm,
            request_id: Some(request),
            fault: WorkerFault::Protocol,
        };
        match &faulted {
            DomainEvent::WorkerFaulted {
                worker,
                request_id,
                fault,
            } => {
                assert_eq!(*worker, WorkerKind::Llm);
                assert_eq!(*request_id, Some(request));
                assert_eq!(*fault, WorkerFault::Protocol);
            }
            _ => unreachable!("the test constructs a worker-fault event"),
        }
        assert_ne!(
            faulted,
            DomainEvent::WorkerFaulted {
                worker: WorkerKind::Llm,
                request_id: None,
                fault: WorkerFault::Protocol,
            }
        );

        match (DomainEvent::RequestCancellationCompleted {
            generation_epoch: generation_epoch(4),
            request_id: request,
        }) {
            DomainEvent::RequestCancellationCompleted {
                generation_epoch: event_generation_epoch,
                request_id,
            } => {
                assert_eq!(event_generation_epoch, generation_epoch(4));
                assert_eq!(request_id, request);
            }
            _ => unreachable!("the test constructs a cancellation outcome"),
        }

        match (DomainEvent::WorkerStopped {
            worker: WorkerKind::Tts,
        }) {
            DomainEvent::WorkerStopped { worker } => {
                assert_eq!(worker, WorkerKind::Tts);
            }
            _ => unreachable!("the test constructs a worker-stop outcome"),
        }

        match (DomainEvent::AudioStopped {
            audio_epoch: Some(audio),
        }) {
            DomainEvent::AudioStopped { audio_epoch } => {
                assert_eq!(audio_epoch, Some(audio));
            }
            _ => unreachable!("the test constructs an audio-stop outcome"),
        }
        assert_ne!(
            DomainEvent::AudioStopped {
                audio_epoch: Some(audio)
            },
            DomainEvent::AudioStopped { audio_epoch: None }
        );
    }

    #[test]
    fn shutdown_events_are_distinct_and_debug_is_deterministic() {
        assert_ne!(
            DomainEvent::ShutdownRequested,
            DomainEvent::ShutdownCompleted
        );
        assert_eq!(
            format!("{:?}", DomainEvent::ShutdownRequested),
            "ShutdownRequested"
        );
        assert_eq!(
            format!(
                "{:?}",
                DomainEvent::WorkerStopped {
                    worker: WorkerKind::Asr
                }
            ),
            "WorkerStopped { worker: Asr }"
        );
    }
}
