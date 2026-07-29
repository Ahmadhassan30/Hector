use hector_core::{GenerationEpoch, RequestId};
use hector_protocol::{
    AudioEncoding, AudioFormat, AudioPayload, Envelope, FaultCode, FinalOutput,
    GenerationCorrelation, Output, OutputChunk, OutputCompletionReason, OutputPayload,
    ProtocolMessage, WorkerFaultMessage, WorkerRole,
};

use crate::ScriptError;

/// One deterministic built-in fake-worker scenario.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FakeScenario {
    Normal,
    PartialThenComplete,
    Cancelled,
    LateOutputAfterCancel,
    AssociatedFault,
    UnassociatedFault,
    ReorderedChunks,
    StaleGeneration,
    FutureGeneration,
    RequestMismatch,
    MalformedFrame,
    UnsupportedVersion,
    Hang,
    Crash,
}

impl FakeScenario {
    /// Parses one exact lowercase built-in scenario name.
    pub fn from_name(name: &str) -> Result<Self, ScriptError> {
        match name {
            "normal" => Ok(Self::Normal),
            "partial_then_complete" => Ok(Self::PartialThenComplete),
            "cancelled" => Ok(Self::Cancelled),
            "late_output_after_cancel" => Ok(Self::LateOutputAfterCancel),
            "associated_fault" => Ok(Self::AssociatedFault),
            "unassociated_fault" => Ok(Self::UnassociatedFault),
            "reordered_chunks" => Ok(Self::ReorderedChunks),
            "stale_generation" => Ok(Self::StaleGeneration),
            "future_generation" => Ok(Self::FutureGeneration),
            "request_mismatch" => Ok(Self::RequestMismatch),
            "malformed_frame" => Ok(Self::MalformedFrame),
            "unsupported_version" => Ok(Self::UnsupportedVersion),
            "hang" => Ok(Self::Hang),
            "crash" => Ok(Self::Crash),
            _ => Err(ScriptError::UnknownScenario),
        }
    }

    /// Returns the exact lowercase CLI name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::PartialThenComplete => "partial_then_complete",
            Self::Cancelled => "cancelled",
            Self::LateOutputAfterCancel => "late_output_after_cancel",
            Self::AssociatedFault => "associated_fault",
            Self::UnassociatedFault => "unassociated_fault",
            Self::ReorderedChunks => "reordered_chunks",
            Self::StaleGeneration => "stale_generation",
            Self::FutureGeneration => "future_generation",
            Self::RequestMismatch => "request_mismatch",
            Self::MalformedFrame => "malformed_frame",
            Self::UnsupportedVersion => "unsupported_version",
            Self::Hang => "hang",
            Self::Crash => "crash",
        }
    }
}

/// Immutable worker role and scenario selection.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FakeWorkerConfig {
    role: WorkerRole,
    scenario: FakeScenario,
}

impl FakeWorkerConfig {
    pub const fn new(role: WorkerRole, scenario: FakeScenario) -> Self {
        Self { role, scenario }
    }

    pub const fn role(&self) -> WorkerRole {
        self.role
    }

    pub const fn scenario(&self) -> FakeScenario {
        self.scenario
    }
}

/// Private raw-output selections owned by explicit adversarial scenarios.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MalformedFrameKind {
    MalformedJson,
    UnsupportedVersion,
}

/// What the process shell must do after emitting typed messages.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FakeDisposition {
    Continue,
    TerminateCleanly,
    EmitMalformedFrame(MalformedFrameKind),
    Hang,
    Crash,
}

/// Typed protocol emissions plus one process-shell disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FakeWorkerResult {
    emissions: Vec<Envelope>,
    disposition: FakeDisposition,
}

impl FakeWorkerResult {
    pub(crate) const fn new(emissions: Vec<Envelope>, disposition: FakeDisposition) -> Self {
        Self {
            emissions,
            disposition,
        }
    }

    pub fn emissions(&self) -> &[Envelope] {
        &self.emissions
    }

    pub(crate) const fn disposition(&self) -> FakeDisposition {
        self.disposition
    }

    pub fn into_parts(self) -> (Vec<Envelope>, FakeDisposition) {
        (self.emissions, self.disposition)
    }
}

pub(crate) fn work_matches_role(role: WorkerRole, work: &hector_protocol::WorkPayload) -> bool {
    matches!(
        (role, work),
        (WorkerRole::Asr, hector_protocol::WorkPayload::Asr { .. })
            | (WorkerRole::Llm, hector_protocol::WorkPayload::Llm { .. })
            | (WorkerRole::Tts, hector_protocol::WorkPayload::Tts { .. })
    )
}

pub(crate) fn ready_emissions(config: FakeWorkerConfig) -> Vec<Envelope> {
    let mut emissions = vec![Envelope::new(ProtocolMessage::Ready {
        worker: config.role(),
    })];
    if config.scenario() == FakeScenario::UnassociatedFault {
        emissions.push(fault(config.role(), None, FaultCode::Unavailable));
    }
    emissions
}

pub(crate) fn immediate_work_result(
    config: FakeWorkerConfig,
    correlation: GenerationCorrelation,
) -> Result<FakeWorkerResult, ScriptError> {
    let result = match config.scenario() {
        FakeScenario::Normal | FakeScenario::UnassociatedFault => FakeWorkerResult::new(
            vec![final_output(
                config.role(),
                correlation,
                0,
                SampleOutput::Final,
                OutputCompletionReason::Completed,
            )?],
            FakeDisposition::Continue,
        ),
        FakeScenario::PartialThenComplete => FakeWorkerResult::new(
            vec![
                partial_output(config.role(), correlation, 0, SampleOutput::Partial0)?,
                final_output(
                    config.role(),
                    correlation,
                    1,
                    SampleOutput::Final,
                    OutputCompletionReason::Completed,
                )?,
            ],
            FakeDisposition::Continue,
        ),
        FakeScenario::AssociatedFault => FakeWorkerResult::new(
            vec![
                fault(config.role(), Some(correlation), FaultCode::InvalidResponse),
                final_output(
                    config.role(),
                    correlation,
                    0,
                    SampleOutput::Faulted,
                    OutputCompletionReason::Faulted,
                )?,
            ],
            FakeDisposition::Continue,
        ),
        FakeScenario::ReorderedChunks => FakeWorkerResult::new(
            vec![
                partial_output(config.role(), correlation, 1, SampleOutput::Partial1)?,
                partial_output(config.role(), correlation, 0, SampleOutput::Partial0)?,
                final_output(
                    config.role(),
                    correlation,
                    2,
                    SampleOutput::Final,
                    OutputCompletionReason::Completed,
                )?,
            ],
            FakeDisposition::Continue,
        ),
        FakeScenario::StaleGeneration => {
            let incoming = correlation.generation_epoch().get();
            let stale = incoming
                .checked_sub(1)
                .and_then(GenerationEpoch::from_raw)
                .ok_or(ScriptError::InvalidGenerationBoundary)?;
            let injected = GenerationCorrelation::new(stale, correlation.request_id());
            FakeWorkerResult::new(
                vec![final_output(
                    config.role(),
                    injected,
                    0,
                    SampleOutput::Final,
                    OutputCompletionReason::Completed,
                )?],
                FakeDisposition::Continue,
            )
        }
        FakeScenario::FutureGeneration => {
            let future = correlation
                .generation_epoch()
                .checked_next()
                .ok_or(ScriptError::InvalidGenerationBoundary)?;
            let injected = GenerationCorrelation::new(future, correlation.request_id());
            FakeWorkerResult::new(
                vec![final_output(
                    config.role(),
                    injected,
                    0,
                    SampleOutput::Final,
                    OutputCompletionReason::Completed,
                )?],
                FakeDisposition::Continue,
            )
        }
        FakeScenario::RequestMismatch => {
            let incoming = correlation.request_id().get();
            let different = if incoming == u128::MAX {
                1
            } else {
                incoming + 1
            };
            let request =
                RequestId::from_raw(different).ok_or(ScriptError::InvalidRequestBoundary)?;
            let injected = GenerationCorrelation::new(correlation.generation_epoch(), request);
            FakeWorkerResult::new(
                vec![final_output(
                    config.role(),
                    injected,
                    0,
                    SampleOutput::Final,
                    OutputCompletionReason::Completed,
                )?],
                FakeDisposition::Continue,
            )
        }
        FakeScenario::MalformedFrame => FakeWorkerResult::new(
            Vec::new(),
            FakeDisposition::EmitMalformedFrame(MalformedFrameKind::MalformedJson),
        ),
        FakeScenario::UnsupportedVersion => FakeWorkerResult::new(
            Vec::new(),
            FakeDisposition::EmitMalformedFrame(MalformedFrameKind::UnsupportedVersion),
        ),
        FakeScenario::Hang => FakeWorkerResult::new(Vec::new(), FakeDisposition::Hang),
        FakeScenario::Crash => FakeWorkerResult::new(Vec::new(), FakeDisposition::Crash),
        FakeScenario::Cancelled | FakeScenario::LateOutputAfterCancel => {
            unreachable!("active cancellation scenarios are handled by the lifecycle")
        }
    };
    Ok(result)
}

pub(crate) fn late_cancelled_output(
    role: WorkerRole,
    correlation: GenerationCorrelation,
) -> Result<Envelope, ScriptError> {
    final_output(
        role,
        correlation,
        1,
        SampleOutput::CancelledLate,
        OutputCompletionReason::Cancelled,
    )
}

pub(crate) fn initial_late_partial(
    role: WorkerRole,
    correlation: GenerationCorrelation,
) -> Result<Envelope, ScriptError> {
    partial_output(role, correlation, 0, SampleOutput::Partial0)
}

fn partial_output(
    role: WorkerRole,
    correlation: GenerationCorrelation,
    chunk_index: u64,
    value: SampleOutput,
) -> Result<Envelope, ScriptError> {
    Ok(Envelope::new(ProtocolMessage::Output(Output::Partial(
        OutputChunk::new(correlation, chunk_index, output_payload(role, value)?),
    ))))
}

fn final_output(
    role: WorkerRole,
    correlation: GenerationCorrelation,
    chunk_index: u64,
    value: SampleOutput,
    reason: OutputCompletionReason,
) -> Result<Envelope, ScriptError> {
    let chunk = OutputChunk::new(correlation, chunk_index, output_payload(role, value)?);
    Ok(Envelope::new(ProtocolMessage::Output(Output::Final(
        FinalOutput::new(chunk, reason),
    ))))
}

fn fault(
    role: WorkerRole,
    association: Option<GenerationCorrelation>,
    code: FaultCode,
) -> Envelope {
    Envelope::new(ProtocolMessage::WorkerFault(WorkerFaultMessage::new(
        role,
        association,
        code,
    )))
}

#[derive(Copy, Clone)]
enum SampleOutput {
    Partial0,
    Partial1,
    Final,
    Faulted,
    CancelledLate,
}

fn output_payload(role: WorkerRole, value: SampleOutput) -> Result<OutputPayload, ScriptError> {
    match role {
        WorkerRole::Asr => Ok(OutputPayload::Asr {
            transcript: sample_text("asr", value),
        }),
        WorkerRole::Llm => Ok(OutputPayload::Llm {
            text: sample_text("llm", value),
        }),
        WorkerRole::Tts => {
            let bytes = match value {
                SampleOutput::Partial0 => [0x00, 0x00],
                SampleOutput::Partial1 => [0x01, 0x00],
                SampleOutput::Final => [0x02, 0x00],
                SampleOutput::Faulted => [0xff, 0xff],
                SampleOutput::CancelledLate => [0x00, 0x80],
            };
            let format = AudioFormat::new(16_000, 1).ok_or(ScriptError::InvalidTypedPayload)?;
            let audio = AudioPayload::new(AudioEncoding::PcmS16Le, format, bytes.to_vec())
                .ok_or(ScriptError::InvalidTypedPayload)?;
            Ok(OutputPayload::Tts { audio })
        }
    }
}

fn sample_text(role: &str, value: SampleOutput) -> String {
    let suffix = match value {
        SampleOutput::Partial0 => "partial-0",
        SampleOutput::Partial1 => "partial-1",
        SampleOutput::Final => "final",
        SampleOutput::Faulted => "faulted",
        SampleOutput::CancelledLate => "cancelled-late",
    };
    format!("fake-{role}-{suffix}")
}

#[cfg(test)]
mod tests {
    use hector_core::{GenerationEpoch, RequestId};
    use hector_protocol::{
        FaultCode, GenerationCorrelation, Output, OutputCompletionReason, OutputPayload,
        ProtocolMessage, WorkerRole,
    };

    use super::{
        FakeDisposition, FakeScenario, FakeWorkerConfig, MalformedFrameKind, immediate_work_result,
    };

    fn correlation(generation: u64, request: u128) -> GenerationCorrelation {
        GenerationCorrelation::new(
            GenerationEpoch::from_raw(generation).expect("nonzero generation"),
            RequestId::from_raw(request).expect("nonzero request"),
        )
    }

    #[test]
    fn every_scenario_name_round_trips_and_unknown_names_fail() {
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
        for scenario in scenarios {
            assert_eq!(FakeScenario::from_name(scenario.name()), Ok(scenario));
        }
        assert!(FakeScenario::from_name("NORMAL").is_err());
        assert!(FakeScenario::from_name("unknown").is_err());
    }

    #[test]
    fn deterministic_immediate_scenarios_have_exact_sequences_for_every_role() {
        for role in [WorkerRole::Asr, WorkerRole::Llm, WorkerRole::Tts] {
            let correlation = correlation(7, 11);
            let partial = immediate_work_result(
                FakeWorkerConfig::new(role, FakeScenario::PartialThenComplete),
                correlation,
            )
            .expect("built-in payloads are valid");
            assert_eq!(partial.emissions().len(), 2);
            assert!(matches!(
                partial.emissions()[0].message(),
                ProtocolMessage::Output(Output::Partial(chunk))
                    if chunk.correlation() == correlation && chunk.chunk_index() == 0
            ));
            assert!(matches!(
                partial.emissions()[1].message(),
                ProtocolMessage::Output(Output::Final(output))
                    if output.chunk().correlation() == correlation
                        && output.chunk().chunk_index() == 1
                        && output.reason() == OutputCompletionReason::Completed
            ));

            let reordered = immediate_work_result(
                FakeWorkerConfig::new(role, FakeScenario::ReorderedChunks),
                correlation,
            )
            .expect("built-in payloads are valid");
            let indexes = reordered
                .emissions()
                .iter()
                .map(|envelope| match envelope.message() {
                    ProtocolMessage::Output(Output::Partial(chunk)) => chunk.chunk_index(),
                    ProtocolMessage::Output(Output::Final(output)) => output.chunk().chunk_index(),
                    _ => panic!("scenario emits only output"),
                })
                .collect::<Vec<_>>();
            assert_eq!(indexes, vec![1, 0, 2]);
        }
    }

    #[test]
    fn injected_correlations_change_exactly_one_identity_dimension() {
        let original = correlation(8, 13);
        let stale = immediate_work_result(
            FakeWorkerConfig::new(WorkerRole::Llm, FakeScenario::StaleGeneration),
            original,
        )
        .expect("generation permits decrement");
        let future = immediate_work_result(
            FakeWorkerConfig::new(WorkerRole::Llm, FakeScenario::FutureGeneration),
            original,
        )
        .expect("generation permits increment");
        let mismatch = immediate_work_result(
            FakeWorkerConfig::new(WorkerRole::Llm, FakeScenario::RequestMismatch),
            original,
        )
        .expect("request can differ");

        let emitted = |result: &super::FakeWorkerResult| match result.emissions()[0].message() {
            ProtocolMessage::Output(Output::Final(output)) => output.chunk().correlation(),
            _ => panic!("expected final output"),
        };
        assert_eq!(emitted(&stale).generation_epoch().get(), 7);
        assert_eq!(emitted(&stale).request_id(), original.request_id());
        assert_eq!(emitted(&future).generation_epoch().get(), 9);
        assert_eq!(emitted(&future).request_id(), original.request_id());
        assert_eq!(
            emitted(&mismatch).generation_epoch(),
            original.generation_epoch()
        );
        assert_ne!(emitted(&mismatch).request_id(), original.request_id());
    }

    #[test]
    fn boundary_preconditions_and_raw_dispositions_are_deterministic() {
        assert!(
            immediate_work_result(
                FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::StaleGeneration),
                correlation(1, 1),
            )
            .is_err()
        );
        assert!(
            immediate_work_result(
                FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::FutureGeneration),
                correlation(u64::MAX, 1),
            )
            .is_err()
        );

        for (scenario, expected) in [
            (
                FakeScenario::MalformedFrame,
                FakeDisposition::EmitMalformedFrame(MalformedFrameKind::MalformedJson),
            ),
            (
                FakeScenario::UnsupportedVersion,
                FakeDisposition::EmitMalformedFrame(MalformedFrameKind::UnsupportedVersion),
            ),
            (FakeScenario::Hang, FakeDisposition::Hang),
            (FakeScenario::Crash, FakeDisposition::Crash),
        ] {
            let (_, disposition) = immediate_work_result(
                FakeWorkerConfig::new(WorkerRole::Tts, scenario),
                correlation(2, 3),
            )
            .expect("scenario is valid")
            .into_parts();
            assert_eq!(disposition, expected);
        }
    }

    #[test]
    fn normal_payloads_are_exact_for_every_role() {
        let correlation = correlation(2, 3);
        for role in [WorkerRole::Asr, WorkerRole::Llm, WorkerRole::Tts] {
            let result = immediate_work_result(
                FakeWorkerConfig::new(role, FakeScenario::Normal),
                correlation,
            )
            .expect("normal payload is valid");
            let ProtocolMessage::Output(Output::Final(output)) = result.emissions()[0].message()
            else {
                panic!("normal scenario emits one final output");
            };
            assert_eq!(output.chunk().correlation(), correlation);
            assert_eq!(output.chunk().chunk_index(), 0);
            assert_eq!(output.reason(), OutputCompletionReason::Completed);
            match output.chunk().output() {
                OutputPayload::Asr { transcript } => {
                    assert_eq!(role, WorkerRole::Asr);
                    assert_eq!(transcript, "fake-asr-final");
                }
                OutputPayload::Llm { text } => {
                    assert_eq!(role, WorkerRole::Llm);
                    assert_eq!(text, "fake-llm-final");
                }
                OutputPayload::Tts { audio } => {
                    assert_eq!(role, WorkerRole::Tts);
                    assert_eq!(audio.format().sample_rate_hz(), 16_000);
                    assert_eq!(audio.format().channels(), 1);
                    assert_eq!(audio.sample_data(), [0x02, 0x00]);
                }
            }
        }
    }

    #[test]
    fn associated_fault_is_exact_and_request_maximum_wraps_to_distinct_one() {
        let correlation = correlation(4, u128::MAX);
        let faulted = immediate_work_result(
            FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::AssociatedFault),
            correlation,
        )
        .expect("fault scenario");
        assert!(matches!(
            faulted.emissions()[0].message(),
            ProtocolMessage::WorkerFault(fault)
                if fault.worker() == WorkerRole::Asr
                    && fault.association() == Some(correlation)
                    && fault.fault() == FaultCode::InvalidResponse
        ));
        assert!(matches!(
            faulted.emissions()[1].message(),
            ProtocolMessage::Output(Output::Final(output))
                if output.reason() == OutputCompletionReason::Faulted
        ));

        let mismatch = immediate_work_result(
            FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::RequestMismatch),
            correlation,
        )
        .expect("maximum request has deterministic alternate");
        assert!(matches!(
            mismatch.emissions()[0].message(),
            ProtocolMessage::Output(Output::Final(output))
                if output.chunk().correlation().request_id().get() == 1
        ));
    }
}
