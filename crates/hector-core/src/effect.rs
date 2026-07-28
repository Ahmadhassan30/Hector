use crate::{AudioEpoch, RequestId, WorkerKind};

/// Control-plane work requested by the pure domain reducer.
///
/// Infrastructure executes an effect and returns its completion as a
/// [`crate::DomainEvent`]. Constructing an effect does not imply that the work
/// has completed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Cancel a request.
    ///
    /// Completion returns
    /// [`crate::DomainEvent::RequestCancellationCompleted`].
    CancelRequest { request_id: RequestId },
    /// Stop a worker.
    ///
    /// Completion returns [`crate::DomainEvent::WorkerStopped`].
    StopWorker { worker: WorkerKind },
    /// Stop audio associated with the observed epoch, if one exists.
    ///
    /// Completion returns [`crate::DomainEvent::AudioStopped`].
    StopAudio { audio_epoch: Option<AudioEpoch> },
    /// Complete application shutdown after preceding work has finished.
    ///
    /// Completion returns [`crate::DomainEvent::ShutdownCompleted`].
    CompleteShutdown,
}

#[cfg(test)]
mod tests {
    use super::Effect;
    use crate::{AudioEpoch, DomainEvent, RequestId, WorkerKind};

    fn request_id(raw: u128) -> RequestId {
        RequestId::from_raw(raw).expect("test request identifiers are nonzero")
    }

    fn audio_epoch(raw: u64) -> AudioEpoch {
        AudioEpoch::from_raw(raw).expect("test audio epochs are nonzero")
    }

    fn assert_correspondence(effect: &Effect, outcome: &DomainEvent) {
        match (effect, outcome) {
            (
                Effect::CancelRequest { request_id: effect },
                DomainEvent::RequestCancellationCompleted { request_id: event },
            ) => assert_eq!(effect, event),
            (
                Effect::StopWorker { worker: effect },
                DomainEvent::WorkerStopped { worker: event },
            ) => assert_eq!(effect, event),
            (
                Effect::StopAudio {
                    audio_epoch: effect,
                },
                DomainEvent::AudioStopped { audio_epoch: event },
            ) => assert_eq!(effect, event),
            (Effect::CompleteShutdown, DomainEvent::ShutdownCompleted) => {}
            _ => panic!("effect and outcome event do not correspond"),
        }
    }

    #[test]
    fn effects_preserve_payloads_and_have_typed_outcomes() {
        let request = request_id(1);
        let audio = audio_epoch(2);

        let cancellation = Effect::CancelRequest {
            request_id: request,
        };
        let stop_worker = Effect::StopWorker {
            worker: WorkerKind::Llm,
        };
        let stop_audio = Effect::StopAudio {
            audio_epoch: Some(audio),
        };
        let complete_shutdown = Effect::CompleteShutdown;

        assert_correspondence(
            &cancellation,
            &DomainEvent::RequestCancellationCompleted {
                request_id: request,
            },
        );
        assert_correspondence(
            &stop_worker,
            &DomainEvent::WorkerStopped {
                worker: WorkerKind::Llm,
            },
        );
        assert_correspondence(
            &stop_audio,
            &DomainEvent::AudioStopped {
                audio_epoch: Some(audio),
            },
        );
        assert_correspondence(&complete_shutdown, &DomainEvent::ShutdownCompleted);
    }

    #[test]
    fn optional_audio_epoch_and_debug_output_are_deterministic() {
        assert_ne!(
            Effect::StopAudio {
                audio_epoch: Some(audio_epoch(1))
            },
            Effect::StopAudio { audio_epoch: None }
        );
        assert_eq!(
            format!(
                "{:?}",
                Effect::CancelRequest {
                    request_id: request_id(7)
                }
            ),
            "CancelRequest { request_id: RequestId(7) }"
        );
        assert_eq!(
            format!("{:?}", Effect::CompleteShutdown),
            "CompleteShutdown"
        );
    }
}
