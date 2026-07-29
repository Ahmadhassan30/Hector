use core::num::{NonZeroU16, NonZeroU32};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hector_core::{AudioEpoch, GenerationEpoch, RequestId, WorkerFault, WorkerKind};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{PROTOCOL_VERSION, ProtocolError};

/// One versioned protocol message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Envelope {
    protocol_version: u16,
    message: ProtocolMessage,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeWire {
    protocol_version: u16,
    message: ProtocolMessage,
}

impl<'de> Deserialize<'de> for Envelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EnvelopeWire::deserialize(deserializer)?;
        if wire.protocol_version != PROTOCOL_VERSION {
            return Err(D::Error::custom("unsupported protocol version"));
        }

        Ok(Self {
            protocol_version: wire.protocol_version,
            message: wire.message,
        })
    }
}

impl Envelope {
    /// Wraps a message in the current protocol version.
    pub const fn new(message: ProtocolMessage) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message,
        }
    }

    /// Returns the envelope's protocol version.
    pub const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    /// Returns the typed message.
    pub const fn message(&self) -> &ProtocolMessage {
        &self.message
    }

    /// Consumes the envelope and returns its typed message.
    pub fn into_message(self) -> ProtocolMessage {
        self.message
    }
}

/// Version-1 host/worker message vocabulary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ProtocolMessage {
    /// The host identifies the worker endpoint.
    Hello { worker: WorkerRole },
    /// The worker accepts the lifecycle handshake.
    Ready { worker: WorkerRole },
    /// The host submits typed, generation-qualified work.
    Work(WorkSubmission),
    /// The worker emits a typed partial or terminal output.
    Output(Output),
    /// The host requests exact generation/request cancellation.
    Cancel { correlation: GenerationCorrelation },
    /// The worker acknowledges the exact cancellation target.
    CancellationAcknowledged { correlation: GenerationCorrelation },
    /// The host requests unconditional worker shutdown.
    Shutdown,
    /// The worker reports that it stopped.
    Stopped { worker: WorkerRole },
    /// The worker reports a typed fault.
    WorkerFault(WorkerFaultMessage),
}

/// A supervised process-worker role.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerRole {
    /// Automatic speech recognition.
    Asr,
    /// Language-model generation.
    Llm,
    /// Text-to-speech synthesis.
    Tts,
}

impl From<WorkerRole> for WorkerKind {
    fn from(value: WorkerRole) -> Self {
        match value {
            WorkerRole::Asr => Self::Asr,
            WorkerRole::Llm => Self::Llm,
            WorkerRole::Tts => Self::Tts,
        }
    }
}

impl TryFrom<WorkerKind> for WorkerRole {
    type Error = ProtocolError;

    fn try_from(value: WorkerKind) -> Result<Self, Self::Error> {
        match value {
            WorkerKind::Asr => Ok(Self::Asr),
            WorkerKind::Llm => Ok(Self::Llm),
            WorkerKind::Tts => Ok(Self::Tts),
            WorkerKind::Audio => Err(ProtocolError::UnsupportedWorkerKind { worker: value }),
        }
    }
}

/// Exact assistant generation and request correlation.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationCorrelation {
    #[serde(with = "generation_epoch_wire")]
    generation_epoch: GenerationEpoch,
    #[serde(with = "request_id_wire")]
    request_id: RequestId,
}

impl GenerationCorrelation {
    /// Creates an exact generation/request association.
    pub const fn new(generation_epoch: GenerationEpoch, request_id: RequestId) -> Self {
        Self {
            generation_epoch,
            request_id,
        }
    }

    /// Returns the sole assistant-output freshness fence.
    pub const fn generation_epoch(self) -> GenerationEpoch {
        self.generation_epoch
    }

    /// Returns the tracing and exact acknowledgement identity.
    pub const fn request_id(self) -> RequestId {
        self.request_id
    }
}

/// One generation-qualified unit of worker input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSubmission {
    correlation: GenerationCorrelation,
    work: WorkPayload,
}

impl WorkSubmission {
    /// Creates a typed work submission.
    pub const fn new(correlation: GenerationCorrelation, work: WorkPayload) -> Self {
        Self { correlation, work }
    }

    /// Returns the exact generation/request association.
    pub const fn correlation(&self) -> GenerationCorrelation {
        self.correlation
    }

    /// Returns the role-specific work.
    pub const fn work(&self) -> &WorkPayload {
        &self.work
    }

    /// Consumes the submission.
    pub fn into_parts(self) -> (GenerationCorrelation, WorkPayload) {
        (self.correlation, self.work)
    }
}

/// Role-specific worker input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "role",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WorkPayload {
    /// Audio belonging to one continuity epoch for transcription.
    Asr {
        #[serde(with = "audio_epoch_wire")]
        audio_epoch: AudioEpoch,
        audio: AudioPayload,
    },
    /// Budgeted language-model input.
    Llm { prompt: String },
    /// Text to synthesize.
    Tts { text: String },
}

/// A partial or terminal worker output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Output {
    /// More chunks may follow.
    Partial(OutputChunk),
    /// This stream has reached a typed terminal reason.
    Final(FinalOutput),
}

/// One typed output chunk.
///
/// `chunk_index` is zero-based and monotonically increasing only within one
/// `(GenerationEpoch, RequestId)` stream. H13 retains the index but performs no
/// stateful replay or sequence acceptance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputChunk {
    correlation: GenerationCorrelation,
    chunk_index: u64,
    output: OutputPayload,
}

impl OutputChunk {
    /// Creates a correlated output chunk.
    pub const fn new(
        correlation: GenerationCorrelation,
        chunk_index: u64,
        output: OutputPayload,
    ) -> Self {
        Self {
            correlation,
            chunk_index,
            output,
        }
    }

    /// Returns the exact generation/request association.
    pub const fn correlation(&self) -> GenerationCorrelation {
        self.correlation
    }

    /// Returns the stream-local, zero-based chunk index.
    pub const fn chunk_index(&self) -> u64 {
        self.chunk_index
    }

    /// Returns the role-specific output.
    pub const fn output(&self) -> &OutputPayload {
        &self.output
    }

    /// Consumes the chunk.
    pub fn into_parts(self) -> (GenerationCorrelation, u64, OutputPayload) {
        (self.correlation, self.chunk_index, self.output)
    }
}

/// One terminal output chunk and its completion reason.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalOutput {
    chunk: OutputChunk,
    reason: OutputCompletionReason,
}

impl FinalOutput {
    /// Creates a terminal output.
    pub const fn new(chunk: OutputChunk, reason: OutputCompletionReason) -> Self {
        Self { chunk, reason }
    }

    /// Returns the terminal chunk.
    pub const fn chunk(&self) -> &OutputChunk {
        &self.chunk
    }

    /// Returns why the stream ended.
    pub const fn reason(&self) -> OutputCompletionReason {
        self.reason
    }

    /// Consumes the terminal output.
    pub fn into_parts(self) -> (OutputChunk, OutputCompletionReason) {
        (self.chunk, self.reason)
    }
}

/// Role-specific worker output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "role",
    content = "payload",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OutputPayload {
    /// Recognized text.
    Asr { transcript: String },
    /// Generated text.
    Llm { text: String },
    /// Synthesized audio.
    Tts { audio: AudioPayload },
}

/// Why a final output stream ended.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputCompletionReason {
    /// The worker completed normally.
    Completed,
    /// The worker ended because the exact request was cancelled.
    Cancelled,
    /// The worker ended because of a separately reported fault.
    Faulted,
}

/// Sample encoding used by a typed audio payload.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioEncoding {
    /// Signed little-endian 16-bit pulse-code modulation.
    PcmS16Le,
}

/// Nonzero audio sample rate and channel count.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioFormat {
    sample_rate_hz: NonZeroU32,
    channels: NonZeroU16,
}

impl AudioFormat {
    /// Constructs a format when both values are nonzero.
    pub const fn new(sample_rate_hz: u32, channels: u16) -> Option<Self> {
        match (NonZeroU32::new(sample_rate_hz), NonZeroU16::new(channels)) {
            (Some(sample_rate_hz), Some(channels)) => Some(Self {
                sample_rate_hz,
                channels,
            }),
            _ => None,
        }
    }

    /// Returns the sample rate in hertz.
    pub const fn sample_rate_hz(self) -> u32 {
        self.sample_rate_hz.get()
    }

    /// Returns the number of interleaved channels.
    pub const fn channels(self) -> u16 {
        self.channels.get()
    }
}

/// Typed audio with canonical Base64 wire data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPayload {
    encoding: AudioEncoding,
    format: AudioFormat,
    sample_data: Vec<u8>,
}

impl AudioPayload {
    /// Constructs audio when bytes contain complete interleaved PCM frames.
    pub fn new(encoding: AudioEncoding, format: AudioFormat, sample_data: Vec<u8>) -> Option<Self> {
        let bytes_per_frame = usize::from(format.channels()).checked_mul(2)?;
        if !sample_data.len().is_multiple_of(bytes_per_frame) {
            return None;
        }

        Some(Self {
            encoding,
            format,
            sample_data,
        })
    }

    /// Returns the sample encoding.
    pub const fn encoding(&self) -> AudioEncoding {
        self.encoding
    }

    /// Returns the audio format.
    pub const fn format(&self) -> AudioFormat {
        self.format
    }

    /// Returns the encoded sample bytes.
    pub fn sample_data(&self) -> &[u8] {
        &self.sample_data
    }

    /// Consumes the payload.
    pub fn into_parts(self) -> (AudioEncoding, AudioFormat, Vec<u8>) {
        (self.encoding, self.format, self.sample_data)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct AudioPayloadWire {
    encoding: AudioEncoding,
    format: AudioFormat,
    sample_data: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioPayloadOwned {
    encoding: AudioEncoding,
    format: AudioFormat,
    sample_data: String,
}

impl Serialize for AudioPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AudioPayloadWire {
            encoding: self.encoding,
            format: self.format,
            sample_data: STANDARD.encode(&self.sample_data),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AudioPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AudioPayloadOwned::deserialize(deserializer)?;
        let sample_data = STANDARD
            .decode(&wire.sample_data)
            .map_err(D::Error::custom)?;
        if STANDARD.encode(&sample_data) != wire.sample_data {
            return Err(D::Error::custom(
                "audio Base64 is not canonical padded form",
            ));
        }

        Self::new(wire.encoding, wire.format, sample_data)
            .ok_or_else(|| D::Error::custom("audio bytes do not contain complete PCM frames"))
    }
}

/// Worker fault data without runtime fatality policy.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerFaultMessage {
    worker: WorkerRole,
    association: Option<GenerationCorrelation>,
    fault: FaultCode,
}

impl WorkerFaultMessage {
    /// Constructs a worker fault with an exact optional association.
    pub const fn new(
        worker: WorkerRole,
        association: Option<GenerationCorrelation>,
        fault: FaultCode,
    ) -> Self {
        Self {
            worker,
            association,
            fault,
        }
    }

    /// Returns the reporting worker.
    pub const fn worker(self) -> WorkerRole {
        self.worker
    }

    /// Returns the exact association, if the fault belongs to a request.
    pub const fn association(self) -> Option<GenerationCorrelation> {
        self.association
    }

    /// Returns the classification without inferring fatality.
    pub const fn fault(self) -> FaultCode {
        self.fault
    }
}

/// Stable wire classification corresponding to a core worker fault.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultCode {
    /// The worker or resource is unavailable.
    Unavailable,
    /// The worker violated the protocol.
    Protocol,
    /// The worker returned an invalid response.
    InvalidResponse,
    /// The exact work ended by cancellation.
    Cancelled,
    /// The worker ended unexpectedly.
    UnexpectedTermination,
}

impl From<WorkerFault> for FaultCode {
    fn from(value: WorkerFault) -> Self {
        match value {
            WorkerFault::Unavailable => Self::Unavailable,
            WorkerFault::Protocol => Self::Protocol,
            WorkerFault::InvalidResponse => Self::InvalidResponse,
            WorkerFault::Cancelled => Self::Cancelled,
            WorkerFault::UnexpectedTermination => Self::UnexpectedTermination,
        }
    }
}

impl From<FaultCode> for WorkerFault {
    fn from(value: FaultCode) -> Self {
        match value {
            FaultCode::Unavailable => Self::Unavailable,
            FaultCode::Protocol => Self::Protocol,
            FaultCode::InvalidResponse => Self::InvalidResponse,
            FaultCode::Cancelled => Self::Cancelled,
            FaultCode::UnexpectedTermination => Self::UnexpectedTermination,
        }
    }
}

mod generation_epoch_wire {
    use hector_core::GenerationEpoch;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(super) fn serialize<S>(value: &GenerationEpoch, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(value.get())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<GenerationEpoch, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u64::deserialize(deserializer)?;
        GenerationEpoch::from_raw(raw)
            .ok_or_else(|| D::Error::custom("generation epoch must be nonzero"))
    }
}

mod request_id_wire {
    use hector_core::RequestId;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(super) fn serialize<S>(value: &RequestId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u128(value.get())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<RequestId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u128::deserialize(deserializer)?;
        RequestId::from_raw(raw).ok_or_else(|| D::Error::custom("request ID must be nonzero"))
    }
}

mod audio_epoch_wire {
    use hector_core::AudioEpoch;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(super) fn serialize<S>(value: &AudioEpoch, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(value.get())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<AudioEpoch, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u64::deserialize(deserializer)?;
        AudioEpoch::from_raw(raw).ok_or_else(|| D::Error::custom("audio epoch must be nonzero"))
    }
}

#[cfg(test)]
mod tests {
    use hector_core::{AudioEpoch, GenerationEpoch, RequestId, WorkerFault, WorkerKind};

    use super::{
        AudioEncoding, AudioFormat, AudioPayload, Envelope, FaultCode, FinalOutput,
        GenerationCorrelation, Output, OutputChunk, OutputCompletionReason, OutputPayload,
        ProtocolMessage, WorkPayload, WorkSubmission, WorkerFaultMessage, WorkerRole,
    };
    use crate::{ProtocolError, decode_frame, encode_frame};

    fn generation(raw: u64) -> GenerationEpoch {
        GenerationEpoch::from_raw(raw).expect("test generations are nonzero")
    }

    fn request(raw: u128) -> RequestId {
        RequestId::from_raw(raw).expect("test requests are nonzero")
    }

    fn audio_epoch(raw: u64) -> AudioEpoch {
        AudioEpoch::from_raw(raw).expect("test audio epochs are nonzero")
    }

    fn correlation() -> GenerationCorrelation {
        GenerationCorrelation::new(generation(3), request(4))
    }

    fn audio() -> AudioPayload {
        AudioPayload::new(
            AudioEncoding::PcmS16Le,
            AudioFormat::new(16_000, 1).expect("valid format"),
            vec![0, 127, 128, 255],
        )
        .expect("two complete mono samples")
    }

    fn round_trip(message: ProtocolMessage) {
        let envelope = Envelope::new(message);
        let frame = encode_frame(&envelope).expect("message encodes");
        assert_eq!(decode_frame(&frame), Ok(envelope));
    }

    #[test]
    fn every_top_level_message_round_trips() {
        let work = WorkSubmission::new(
            correlation(),
            WorkPayload::Llm {
                prompt: "question".to_owned(),
            },
        );
        let output = Output::Partial(OutputChunk::new(
            correlation(),
            0,
            OutputPayload::Llm {
                text: "answer".to_owned(),
            },
        ));
        let fault =
            WorkerFaultMessage::new(WorkerRole::Llm, Some(correlation()), FaultCode::Protocol);

        for message in [
            ProtocolMessage::Hello {
                worker: WorkerRole::Asr,
            },
            ProtocolMessage::Ready {
                worker: WorkerRole::Llm,
            },
            ProtocolMessage::Work(work),
            ProtocolMessage::Output(output),
            ProtocolMessage::Cancel {
                correlation: correlation(),
            },
            ProtocolMessage::CancellationAcknowledged {
                correlation: correlation(),
            },
            ProtocolMessage::Shutdown,
            ProtocolMessage::Stopped {
                worker: WorkerRole::Tts,
            },
            ProtocolMessage::WorkerFault(fault),
        ] {
            round_trip(message);
        }
    }

    #[test]
    fn every_role_payload_and_output_status_round_trips() {
        for work in [
            WorkPayload::Asr {
                audio_epoch: audio_epoch(8),
                audio: audio(),
            },
            WorkPayload::Llm {
                prompt: "prompt".to_owned(),
            },
            WorkPayload::Tts {
                text: "speak".to_owned(),
            },
        ] {
            round_trip(ProtocolMessage::Work(WorkSubmission::new(
                correlation(),
                work,
            )));
        }

        let outputs = [
            OutputPayload::Asr {
                transcript: "heard".to_owned(),
            },
            OutputPayload::Llm {
                text: "thought".to_owned(),
            },
            OutputPayload::Tts { audio: audio() },
        ];
        for (index, output) in outputs.into_iter().enumerate() {
            let chunk = OutputChunk::new(correlation(), index as u64, output);
            round_trip(ProtocolMessage::Output(Output::Partial(chunk.clone())));
            for reason in [
                OutputCompletionReason::Completed,
                OutputCompletionReason::Cancelled,
                OutputCompletionReason::Faulted,
            ] {
                round_trip(ProtocolMessage::Output(Output::Final(FinalOutput::new(
                    chunk.clone(),
                    reason,
                ))));
            }
        }
    }

    #[test]
    fn golden_json_has_fixed_field_order_names_numbers_and_base64() {
        let envelope = Envelope::new(ProtocolMessage::Work(WorkSubmission::new(
            correlation(),
            WorkPayload::Asr {
                audio_epoch: audio_epoch(8),
                audio: audio(),
            },
        )));
        let frame = encode_frame(&envelope).expect("message encodes");
        let json = core::str::from_utf8(&frame[4..]).expect("JSON is UTF-8");

        assert_eq!(
            json,
            r#"{"protocol_version":1,"message":{"type":"work","payload":{"correlation":{"generation_epoch":3,"request_id":4},"work":{"role":"asr","payload":{"audio_epoch":8,"audio":{"encoding":"pcm_s16_le","format":{"sample_rate_hz":16000,"channels":1},"sample_data":"AH+A/w=="}}}}}}"#
        );
        assert_eq!(
            encode_frame(&envelope),
            encode_frame(&envelope),
            "identical values must serialize identically"
        );
    }

    #[test]
    fn identifiers_use_json_numbers_and_preserve_numeric_limits() {
        let envelope = Envelope::new(ProtocolMessage::Cancel {
            correlation: GenerationCorrelation::new(generation(u64::MAX), request(u128::MAX)),
        });
        let frame = encode_frame(&envelope).expect("maximum identifiers encode");
        let json = core::str::from_utf8(&frame[4..]).expect("valid UTF-8");

        assert!(json.contains(&format!(r#""generation_epoch":{}"#, u64::MAX)));
        assert!(json.contains(&format!(r#""request_id":{}"#, u128::MAX)));
        assert_eq!(decode_frame(&frame), Ok(envelope));
    }

    #[test]
    fn audio_format_and_pcm_frames_are_checked() {
        assert_eq!(AudioFormat::new(0, 1), None);
        assert_eq!(AudioFormat::new(16_000, 0), None);
        let format = AudioFormat::new(16_000, 2).expect("valid format");
        assert_eq!(format.sample_rate_hz(), 16_000);
        assert_eq!(format.channels(), 2);
        assert_eq!(
            AudioPayload::new(AudioEncoding::PcmS16Le, format, vec![0, 1, 2]),
            None
        );
    }

    #[test]
    fn invalid_noncanonical_and_non_string_audio_data_are_rejected() {
        let prefix = r#"{"protocol_version":1,"message":{"type":"work","payload":{"correlation":{"generation_epoch":3,"request_id":4},"work":{"role":"asr","payload":{"audio_epoch":8,"audio":{"encoding":"pcm_s16_le","format":{"sample_rate_hz":16000,"channels":1},"sample_data":"__DATA__"}}}}}}"#;

        for data in ["***=", "AA", "AB==", "[0,1]"] {
            let json = if data.starts_with('[') {
                prefix.replace(r#""__DATA__""#, data)
            } else {
                prefix.replace("__DATA__", data)
            };
            let mut frame = Vec::new();
            frame.extend_from_slice(&(json.len() as u32).to_be_bytes());
            frame.extend_from_slice(json.as_bytes());
            assert_eq!(
                decode_frame(&frame),
                Err(ProtocolError::StructurallyInvalidMessage),
                "data {data} must be rejected"
            );
        }
    }

    #[test]
    fn unknown_fields_variants_and_missing_or_zero_ids_are_rejected() {
        let invalid_messages = [
            r#"{"protocol_version":1,"message":{"type":"unknown"}}"#,
            r#"{"protocol_version":1,"extra":1,"message":{"type":"shutdown"}}"#,
            r#"{"protocol_version":1,"message":{"type":"cancel","payload":{"correlation":{"request_id":4}}}}"#,
            r#"{"protocol_version":1,"message":{"type":"cancel","payload":{"correlation":{"generation_epoch":3}}}}"#,
            r#"{"protocol_version":1,"message":{"type":"cancel","payload":{"correlation":{"generation_epoch":0,"request_id":4}}}}"#,
            r#"{"protocol_version":1,"message":{"type":"cancel","payload":{"correlation":{"generation_epoch":3,"request_id":0}}}}"#,
        ];

        for json in invalid_messages {
            let mut frame = Vec::new();
            frame.extend_from_slice(&(json.len() as u32).to_be_bytes());
            frame.extend_from_slice(json.as_bytes());
            assert_eq!(
                decode_frame(&frame),
                Err(ProtocolError::StructurallyInvalidMessage),
                "invalid message: {json}"
            );
        }
    }

    #[test]
    fn chunk_indexes_are_stream_local_data_not_freshness() {
        let first = OutputChunk::new(
            GenerationCorrelation::new(generation(3), request(4)),
            0,
            OutputPayload::Llm {
                text: "a".to_owned(),
            },
        );
        let other_stream = OutputChunk::new(
            GenerationCorrelation::new(generation(4), request(5)),
            0,
            OutputPayload::Llm {
                text: "b".to_owned(),
            },
        );

        assert_eq!(first.chunk_index(), 0);
        assert_eq!(other_stream.chunk_index(), 0);
        assert_ne!(first.correlation(), other_stream.correlation());
    }

    #[test]
    fn worker_and_fault_conversions_preserve_classification_without_policy() {
        for (role, kind) in [
            (WorkerRole::Asr, WorkerKind::Asr),
            (WorkerRole::Llm, WorkerKind::Llm),
            (WorkerRole::Tts, WorkerKind::Tts),
        ] {
            assert_eq!(WorkerKind::from(role), kind);
            assert_eq!(WorkerRole::try_from(kind), Ok(role));
        }
        assert_eq!(
            WorkerRole::try_from(WorkerKind::Audio),
            Err(ProtocolError::UnsupportedWorkerKind {
                worker: WorkerKind::Audio,
            })
        );

        for fault in [
            WorkerFault::Unavailable,
            WorkerFault::Protocol,
            WorkerFault::InvalidResponse,
            WorkerFault::Cancelled,
            WorkerFault::UnexpectedTermination,
        ] {
            assert_eq!(WorkerFault::from(FaultCode::from(fault)), fault);
        }
    }

    #[test]
    fn absent_fault_association_remains_distinct_from_an_exact_association() {
        let unassociated = WorkerFaultMessage::new(WorkerRole::Asr, None, FaultCode::Unavailable);
        let associated =
            WorkerFaultMessage::new(WorkerRole::Asr, Some(correlation()), FaultCode::Unavailable);

        assert_eq!(unassociated.association(), None);
        assert_eq!(associated.association(), Some(correlation()));
        assert_ne!(unassociated, associated);
    }
}
