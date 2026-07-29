//! Bounded, versioned, transport-independent worker protocol for Hector.
//!
//! Protocol version 1 uses length-prefixed JSON. The semantic message types
//! remain separate from framing so a future protocol version can select a
//! different encoding without moving worker semantics into the runtime or
//! domain core.

mod error;
mod frame;
mod message;
mod validation;

pub use error::ProtocolError;
pub use frame::{FrameHeader, MAX_JSON_PAYLOAD_BYTES, decode_frame, decode_payload, encode_frame};
pub use message::{
    AudioEncoding, AudioFormat, AudioPayload, Envelope, FaultCode, FinalOutput,
    GenerationCorrelation, Output, OutputChunk, OutputCompletionReason, OutputPayload,
    ProtocolMessage, WorkPayload, WorkSubmission, WorkerFaultMessage, WorkerRole,
};
pub use validation::{validate_correlation, validate_generation};

/// The only protocol version supported by H13.
pub const PROTOCOL_VERSION: u16 = 1;
