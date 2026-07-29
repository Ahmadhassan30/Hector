use hector_protocol::{
    Envelope, GenerationCorrelation, ProtocolMessage, WorkSubmission, WorkerRole,
};

use crate::{
    FakeDisposition, FakeScenario, FakeWorkerConfig, FakeWorkerResult, LifecycleError, MessageKind,
    ScriptError,
    script::{
        immediate_work_result, initial_late_partial, late_cancelled_output, ready_emissions,
        work_matches_role,
    },
};

/// Strict worker-side protocol lifecycle.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FakeLifecycle {
    AwaitingHello,
    Idle,
    Active,
    Terminated,
}

/// Deterministic fake worker with exactly one active request.
#[derive(Debug)]
pub struct FakeWorker {
    config: FakeWorkerConfig,
    lifecycle: FakeLifecycle,
    active: Option<GenerationCorrelation>,
}

impl FakeWorker {
    /// Creates a worker without performing I/O or allocating model resources.
    pub fn new(config: FakeWorkerConfig) -> Result<Self, ScriptError> {
        Ok(Self {
            config,
            lifecycle: FakeLifecycle::AwaitingHello,
            active: None,
        })
    }

    /// Returns the current protocol lifecycle state.
    pub const fn lifecycle(&self) -> FakeLifecycle {
        self.lifecycle
    }

    /// Handles one already-decoded H13 envelope.
    pub fn handle(&mut self, envelope: Envelope) -> Result<FakeWorkerResult, LifecycleError> {
        let kind = MessageKind::from(envelope.message());
        let message = envelope.into_message();
        match self.lifecycle {
            FakeLifecycle::AwaitingHello => self.handle_awaiting_hello(message, kind),
            FakeLifecycle::Idle => self.handle_idle(message, kind),
            FakeLifecycle::Active => self.handle_active(message, kind),
            FakeLifecycle::Terminated => Err(LifecycleError::UnexpectedMessage {
                state: FakeLifecycle::Terminated,
                message: kind,
            }),
        }
    }

    fn handle_awaiting_hello(
        &mut self,
        message: ProtocolMessage,
        kind: MessageKind,
    ) -> Result<FakeWorkerResult, LifecycleError> {
        match message {
            ProtocolMessage::Hello { worker } if worker == self.config.role() => {
                self.lifecycle = FakeLifecycle::Idle;
                Ok(FakeWorkerResult::new(
                    ready_emissions(self.config),
                    FakeDisposition::Continue,
                ))
            }
            ProtocolMessage::Hello { .. } => Err(LifecycleError::WrongRole),
            _ => Err(LifecycleError::UnexpectedMessage {
                state: FakeLifecycle::AwaitingHello,
                message: kind,
            }),
        }
    }

    fn handle_idle(
        &mut self,
        message: ProtocolMessage,
        kind: MessageKind,
    ) -> Result<FakeWorkerResult, LifecycleError> {
        match message {
            ProtocolMessage::Work(submission) => self.start_work(submission),
            ProtocolMessage::Shutdown => {
                self.lifecycle = FakeLifecycle::Terminated;
                Ok(stopped(self.config.role()))
            }
            ProtocolMessage::Cancel { .. } => Err(LifecycleError::CancelWhileIdle),
            _ => Err(LifecycleError::UnexpectedMessage {
                state: FakeLifecycle::Idle,
                message: kind,
            }),
        }
    }

    fn start_work(
        &mut self,
        submission: WorkSubmission,
    ) -> Result<FakeWorkerResult, LifecycleError> {
        if !work_matches_role(self.config.role(), submission.work()) {
            return Err(LifecycleError::WrongRole);
        }

        let correlation = submission.correlation();
        match self.config.scenario() {
            FakeScenario::Cancelled => {
                self.active = Some(correlation);
                self.lifecycle = FakeLifecycle::Active;
                Ok(FakeWorkerResult::new(Vec::new(), FakeDisposition::Continue))
            }
            FakeScenario::LateOutputAfterCancel => {
                let partial = initial_late_partial(self.config.role(), correlation)
                    .map_err(|_| LifecycleError::ScenarioPrecondition)?;
                self.active = Some(correlation);
                self.lifecycle = FakeLifecycle::Active;
                Ok(FakeWorkerResult::new(
                    vec![partial],
                    FakeDisposition::Continue,
                ))
            }
            _ => {
                let result = immediate_work_result(self.config, correlation)
                    .map_err(|_| LifecycleError::ScenarioPrecondition)?;
                self.lifecycle = match result.disposition() {
                    FakeDisposition::Continue => FakeLifecycle::Idle,
                    FakeDisposition::TerminateCleanly
                    | FakeDisposition::EmitMalformedFrame(_)
                    | FakeDisposition::Hang
                    | FakeDisposition::Crash => FakeLifecycle::Terminated,
                };
                Ok(result)
            }
        }
    }

    fn handle_active(
        &mut self,
        message: ProtocolMessage,
        kind: MessageKind,
    ) -> Result<FakeWorkerResult, LifecycleError> {
        match message {
            ProtocolMessage::Cancel { correlation } => self.cancel(correlation),
            ProtocolMessage::Shutdown => {
                self.active = None;
                self.lifecycle = FakeLifecycle::Terminated;
                Ok(stopped(self.config.role()))
            }
            ProtocolMessage::Work(_) => Err(LifecycleError::WorkAlreadyActive),
            _ => Err(LifecycleError::UnexpectedMessage {
                state: FakeLifecycle::Active,
                message: kind,
            }),
        }
    }

    fn cancel(
        &mut self,
        correlation: GenerationCorrelation,
    ) -> Result<FakeWorkerResult, LifecycleError> {
        if self.active != Some(correlation) {
            return Err(LifecycleError::CorrelationMismatch);
        }

        self.active = None;
        self.lifecycle = FakeLifecycle::Idle;
        let mut emissions = vec![Envelope::new(ProtocolMessage::CancellationAcknowledged {
            correlation,
        })];
        if self.config.scenario() == FakeScenario::LateOutputAfterCancel {
            emissions.push(
                late_cancelled_output(self.config.role(), correlation)
                    .map_err(|_| LifecycleError::ScenarioPrecondition)?,
            );
        }
        Ok(FakeWorkerResult::new(emissions, FakeDisposition::Continue))
    }
}

fn stopped(role: WorkerRole) -> FakeWorkerResult {
    FakeWorkerResult::new(
        vec![Envelope::new(ProtocolMessage::Stopped { worker: role })],
        FakeDisposition::TerminateCleanly,
    )
}

#[cfg(test)]
mod tests {
    use hector_core::{AudioEpoch, GenerationEpoch, RequestId};
    use hector_protocol::{
        AudioEncoding, AudioFormat, AudioPayload, Envelope, GenerationCorrelation, Output,
        OutputCompletionReason, ProtocolMessage, WorkPayload, WorkSubmission, WorkerRole,
    };

    use crate::{
        FakeDisposition, FakeLifecycle, FakeScenario, FakeWorkerConfig, LifecycleError, MessageKind,
    };

    use super::FakeWorker;

    fn correlation(generation: u64, request: u128) -> GenerationCorrelation {
        GenerationCorrelation::new(
            GenerationEpoch::from_raw(generation).expect("nonzero generation"),
            RequestId::from_raw(request).expect("nonzero request"),
        )
    }

    fn work(role: WorkerRole, correlation: GenerationCorrelation) -> Envelope {
        let payload = match role {
            WorkerRole::Asr => WorkPayload::Asr {
                audio_epoch: AudioEpoch::from_raw(1).expect("nonzero audio epoch"),
                audio: AudioPayload::new(
                    AudioEncoding::PcmS16Le,
                    AudioFormat::new(16_000, 1).expect("valid format"),
                    vec![0, 0],
                )
                .expect("complete PCM frame"),
            },
            WorkerRole::Llm => WorkPayload::Llm {
                prompt: "test prompt".to_owned(),
            },
            WorkerRole::Tts => WorkPayload::Tts {
                text: "test text".to_owned(),
            },
        };
        Envelope::new(ProtocolMessage::Work(WorkSubmission::new(
            correlation,
            payload,
        )))
    }

    fn ready_worker(role: WorkerRole, scenario: FakeScenario) -> FakeWorker {
        let mut worker =
            FakeWorker::new(FakeWorkerConfig::new(role, scenario)).expect("valid config");
        let result = worker
            .handle(Envelope::new(ProtocolMessage::Hello { worker: role }))
            .expect("matching hello");
        assert!(matches!(
            result.emissions()[0].message(),
            ProtocolMessage::Ready { worker: emitted } if *emitted == role
        ));
        worker
    }

    #[test]
    fn hello_is_strict_and_unassociated_fault_is_emitted_after_ready() {
        let mut wrong =
            FakeWorker::new(FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::Normal))
                .expect("valid config");
        assert_eq!(
            wrong.handle(Envelope::new(ProtocolMessage::Hello {
                worker: WorkerRole::Llm,
            })),
            Err(LifecycleError::WrongRole)
        );
        assert_eq!(wrong.lifecycle(), FakeLifecycle::AwaitingHello);

        let mut worker = FakeWorker::new(FakeWorkerConfig::new(
            WorkerRole::Tts,
            FakeScenario::UnassociatedFault,
        ))
        .expect("valid config");
        let result = worker
            .handle(Envelope::new(ProtocolMessage::Hello {
                worker: WorkerRole::Tts,
            }))
            .expect("matching hello");
        assert_eq!(result.emissions().len(), 2);
        assert!(matches!(
            result.emissions()[1].message(),
            ProtocolMessage::WorkerFault(fault)
                if fault.worker() == WorkerRole::Tts && fault.association().is_none()
        ));
    }

    #[test]
    fn normal_work_finishes_immediately_for_every_role_and_wrong_role_fails() {
        for role in [WorkerRole::Asr, WorkerRole::Llm, WorkerRole::Tts] {
            let mut worker = ready_worker(role, FakeScenario::Normal);
            let result = worker
                .handle(work(role, correlation(2, 3)))
                .expect("matching work");
            assert_eq!(worker.lifecycle(), FakeLifecycle::Idle);
            assert!(matches!(
                result.emissions()[0].message(),
                ProtocolMessage::Output(Output::Final(output))
                    if output.reason() == OutputCompletionReason::Completed
            ));
        }

        let mut worker = ready_worker(WorkerRole::Asr, FakeScenario::Normal);
        assert_eq!(
            worker.handle(work(WorkerRole::Llm, correlation(2, 3))),
            Err(LifecycleError::WrongRole)
        );
    }

    #[test]
    fn cancellation_requires_exact_active_correlation_and_acknowledges_once() {
        let active = correlation(7, 8);
        let mut worker = ready_worker(WorkerRole::Llm, FakeScenario::Cancelled);
        let start = worker
            .handle(work(WorkerRole::Llm, active))
            .expect("work starts");
        assert!(start.emissions().is_empty());
        assert_eq!(worker.lifecycle(), FakeLifecycle::Active);

        assert_eq!(
            worker.handle(Envelope::new(ProtocolMessage::Cancel {
                correlation: correlation(7, 9),
            })),
            Err(LifecycleError::CorrelationMismatch)
        );
        let acknowledged = worker
            .handle(Envelope::new(ProtocolMessage::Cancel {
                correlation: active,
            }))
            .expect("exact cancellation");
        assert!(matches!(
            acknowledged.emissions(),
            [envelope] if matches!(
                envelope.message(),
                ProtocolMessage::CancellationAcknowledged { correlation } if *correlation == active
            )
        ));
        assert_eq!(worker.lifecycle(), FakeLifecycle::Idle);
        assert_eq!(
            worker.handle(Envelope::new(ProtocolMessage::Cancel {
                correlation: active,
            })),
            Err(LifecycleError::CancelWhileIdle)
        );
    }

    #[test]
    fn late_output_after_cancel_is_explicit_and_correlation_preserving() {
        let active = correlation(4, 5);
        let mut worker = ready_worker(WorkerRole::Asr, FakeScenario::LateOutputAfterCancel);
        let partial = worker
            .handle(work(WorkerRole::Asr, active))
            .expect("work starts");
        assert_eq!(partial.emissions().len(), 1);
        assert_eq!(worker.lifecycle(), FakeLifecycle::Active);

        let cancelled = worker
            .handle(Envelope::new(ProtocolMessage::Cancel {
                correlation: active,
            }))
            .expect("exact cancellation");
        assert_eq!(cancelled.emissions().len(), 2);
        assert!(matches!(
            cancelled.emissions()[1].message(),
            ProtocolMessage::Output(Output::Final(output))
                if output.chunk().correlation() == active
                    && output.reason() == OutputCompletionReason::Cancelled
        ));
    }

    #[test]
    fn active_worker_rejects_second_work_and_shutdown_stops_without_ack_or_final() {
        let active = correlation(2, 2);
        let mut worker = ready_worker(WorkerRole::Tts, FakeScenario::Cancelled);
        worker
            .handle(work(WorkerRole::Tts, active))
            .expect("work starts");
        assert_eq!(
            worker.handle(work(WorkerRole::Tts, correlation(3, 3))),
            Err(LifecycleError::WorkAlreadyActive)
        );

        let stopped = worker
            .handle(Envelope::new(ProtocolMessage::Shutdown))
            .expect("shutdown accepted");
        assert_eq!(worker.lifecycle(), FakeLifecycle::Terminated);
        let (emissions, disposition) = stopped.into_parts();
        assert_eq!(disposition, FakeDisposition::TerminateCleanly);
        assert!(matches!(
            emissions.as_slice(),
            [envelope] if matches!(
                envelope.message(),
                ProtocolMessage::Stopped { worker: WorkerRole::Tts }
            )
        ));
    }

    #[test]
    fn idle_shutdown_and_invalid_messages_follow_strict_lifecycle() {
        let mut worker = ready_worker(WorkerRole::Llm, FakeScenario::Normal);
        assert_eq!(
            worker.handle(Envelope::new(ProtocolMessage::Ready {
                worker: WorkerRole::Llm,
            })),
            Err(LifecycleError::UnexpectedMessage {
                state: FakeLifecycle::Idle,
                message: MessageKind::Ready,
            })
        );
        let stopped = worker
            .handle(Envelope::new(ProtocolMessage::Shutdown))
            .expect("idle shutdown");
        assert_eq!(stopped.emissions().len(), 1);
        assert_eq!(worker.lifecycle(), FakeLifecycle::Terminated);
    }

    #[test]
    fn adversarial_generation_boundaries_fail_without_changing_lifecycle() {
        let mut stale = ready_worker(WorkerRole::Llm, FakeScenario::StaleGeneration);
        assert_eq!(
            stale.handle(work(WorkerRole::Llm, correlation(1, 1))),
            Err(LifecycleError::ScenarioPrecondition)
        );
        assert_eq!(stale.lifecycle(), FakeLifecycle::Idle);

        let mut future = ready_worker(WorkerRole::Llm, FakeScenario::FutureGeneration);
        assert_eq!(
            future.handle(work(WorkerRole::Llm, correlation(u64::MAX, 1))),
            Err(LifecycleError::ScenarioPrecondition)
        );
        assert_eq!(future.lifecycle(), FakeLifecycle::Idle);
    }

    #[test]
    fn every_declared_scenario_accepts_every_worker_role() {
        let scenarios = [
            FakeScenario::Normal,
            FakeScenario::PartialThenComplete,
            FakeScenario::Cancelled,
            FakeScenario::LateOutputAfterCancel,
            FakeScenario::AssociatedFault,
            FakeScenario::UnassociatedFault,
            FakeScenario::ReorderedChunks,
            FakeScenario::StaleGeneration,
            FakeScenario::FutureGeneration,
            FakeScenario::RequestMismatch,
            FakeScenario::MalformedFrame,
            FakeScenario::UnsupportedVersion,
            FakeScenario::Hang,
            FakeScenario::Crash,
        ];
        let active = correlation(2, 3);

        for role in [WorkerRole::Asr, WorkerRole::Llm, WorkerRole::Tts] {
            for scenario in scenarios {
                let mut worker = ready_worker(role, scenario);
                let result = worker
                    .handle(work(role, active))
                    .expect("all scenarios support all roles");
                match scenario {
                    FakeScenario::Cancelled | FakeScenario::LateOutputAfterCancel => {
                        assert_eq!(worker.lifecycle(), FakeLifecycle::Active);
                        worker
                            .handle(Envelope::new(ProtocolMessage::Cancel {
                                correlation: active,
                            }))
                            .expect("active scenario supports exact cancellation");
                        assert_eq!(worker.lifecycle(), FakeLifecycle::Idle);
                    }
                    FakeScenario::MalformedFrame
                    | FakeScenario::UnsupportedVersion
                    | FakeScenario::Hang
                    | FakeScenario::Crash => {
                        assert_eq!(worker.lifecycle(), FakeLifecycle::Terminated);
                        assert_ne!(result.into_parts().1, FakeDisposition::Continue);
                    }
                    _ => assert_eq!(worker.lifecycle(), FakeLifecycle::Idle),
                }
            }
        }
    }
}
