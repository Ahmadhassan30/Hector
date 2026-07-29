use core::fmt;
use hector_core::{GenerationEpoch, RequestId, WorkerKind};

use crate::MAX_JSON_PAYLOAD_BYTES;

/// Deterministic framing, schema, compatibility, and correlation failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    /// The declared or encoded JSON payload exceeds the protocol limit.
    OversizedFrame { size: usize },
    /// A frame declared an empty JSON payload.
    ZeroLengthFrame,
    /// Fewer than four prefix bytes were available.
    TruncatedPrefix { available: usize },
    /// Fewer payload bytes were available than the prefix declared.
    TruncatedPayload { declared: usize, available: usize },
    /// Bytes remained after the single declared frame.
    TrailingBytes { count: usize },
    /// The declared payload was not valid UTF-8.
    InvalidUtf8,
    /// The payload was not syntactically valid JSON.
    InvalidJson,
    /// The envelope version is not supported.
    UnsupportedProtocolVersion { received: u16 },
    /// Syntactically valid JSON did not conform to the version-1 schema.
    StructurallyInvalidMessage,
    /// A valid protocol value could not be serialized.
    SerializationFailure,
    /// Incoming assistant output belongs to an older generation.
    StaleGeneration {
        active: GenerationEpoch,
        incoming: GenerationEpoch,
    },
    /// Incoming assistant output claims a generation not yet active.
    UnexpectedFutureGeneration {
        active: GenerationEpoch,
        incoming: GenerationEpoch,
    },
    /// A current-generation message carried the wrong request identity.
    RequestCorrelationMismatch {
        expected: RequestId,
        incoming: RequestId,
    },
    /// The audio boundary is not a supervised process-worker role.
    UnsupportedWorkerKind { worker: WorkerKind },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OversizedFrame { size } => write!(
                formatter,
                "JSON payload size {size} exceeds the {MAX_JSON_PAYLOAD_BYTES}-byte limit"
            ),
            Self::ZeroLengthFrame => formatter.write_str("zero-length frames are invalid"),
            Self::TruncatedPrefix { available } => {
                write!(formatter, "frame prefix is truncated at {available} bytes")
            }
            Self::TruncatedPayload {
                declared,
                available,
            } => write!(
                formatter,
                "frame payload declares {declared} bytes but only {available} are available"
            ),
            Self::TrailingBytes { count } => {
                write!(
                    formatter,
                    "{count} trailing bytes follow the declared frame"
                )
            }
            Self::InvalidUtf8 => formatter.write_str("frame payload is not valid UTF-8"),
            Self::InvalidJson => formatter.write_str("frame payload is not valid JSON"),
            Self::UnsupportedProtocolVersion { received } => {
                write!(formatter, "protocol version {received} is unsupported")
            }
            Self::StructurallyInvalidMessage => {
                formatter.write_str("JSON does not conform to the protocol schema")
            }
            Self::SerializationFailure => {
                formatter.write_str("protocol value could not be serialized")
            }
            Self::StaleGeneration { active, incoming } => write!(
                formatter,
                "incoming generation {incoming} is stale relative to active generation {active}"
            ),
            Self::UnexpectedFutureGeneration { active, incoming } => write!(
                formatter,
                "incoming generation {incoming} is newer than active generation {active}"
            ),
            Self::RequestCorrelationMismatch { expected, incoming } => write!(
                formatter,
                "incoming request {incoming} does not match expected request {expected}"
            ),
            Self::UnsupportedWorkerKind { worker } => {
                write!(formatter, "{worker:?} is not a process-worker role")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}
