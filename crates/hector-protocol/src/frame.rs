use std::io::{self, Write};

use serde::Deserialize;

use crate::{Envelope, PROTOCOL_VERSION, ProtocolError};

/// Maximum encoded JSON payload size for protocol version 1.
pub const MAX_JSON_PAYLOAD_BYTES: usize = 1_048_576;

/// A validated four-byte frame prefix.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    payload_len: usize,
}

impl FrameHeader {
    /// Decodes and validates exactly one four-byte big-endian prefix.
    pub fn decode(prefix: &[u8]) -> Result<Self, ProtocolError> {
        if prefix.len() < 4 {
            return Err(ProtocolError::TruncatedPrefix {
                available: prefix.len(),
            });
        }
        if prefix.len() > 4 {
            return Err(ProtocolError::TrailingBytes {
                count: prefix.len() - 4,
            });
        }

        let payload_len = u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
        if payload_len == 0 {
            return Err(ProtocolError::ZeroLengthFrame);
        }
        if payload_len > MAX_JSON_PAYLOAD_BYTES {
            return Err(ProtocolError::OversizedFrame { size: payload_len });
        }

        Ok(Self { payload_len })
    }

    /// Returns the validated JSON payload length.
    pub const fn payload_len(&self) -> usize {
        self.payload_len
    }
}

/// Serializes and frames one versioned envelope.
pub fn encode_frame(envelope: &Envelope) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = BoundedWriter::new();
    if serde_json::to_writer(&mut writer, envelope).is_err() {
        return Err(if writer.exceeded {
            ProtocolError::OversizedFrame {
                size: writer.attempted_len,
            }
        } else {
            ProtocolError::SerializationFailure
        });
    }
    let payload = writer.into_bytes();
    if payload.is_empty() {
        return Err(ProtocolError::ZeroLengthFrame);
    }

    let payload_len = u32::try_from(payload.len()).map_err(|_| ProtocolError::OversizedFrame {
        size: payload.len(),
    })?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&payload_len.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decodes a validated payload without performing transport I/O.
pub fn decode_payload(header: FrameHeader, payload: &[u8]) -> Result<Envelope, ProtocolError> {
    if payload.len() < header.payload_len {
        return Err(ProtocolError::TruncatedPayload {
            declared: header.payload_len,
            available: payload.len(),
        });
    }
    if payload.len() > header.payload_len {
        return Err(ProtocolError::TrailingBytes {
            count: payload.len() - header.payload_len,
        });
    }

    let json = core::str::from_utf8(payload).map_err(|_| ProtocolError::InvalidUtf8)?;
    serde_json::from_str::<serde::de::IgnoredAny>(json).map_err(|_| ProtocolError::InvalidJson)?;
    let probe = serde_json::from_str::<VersionProbe>(json)
        .map_err(|_| ProtocolError::StructurallyInvalidMessage)?;
    if probe.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedProtocolVersion {
            received: probe.protocol_version,
        });
    }

    serde_json::from_str(json).map_err(|_| ProtocolError::StructurallyInvalidMessage)
}

/// Decodes exactly one complete frame without performing transport I/O.
pub fn decode_frame(frame: &[u8]) -> Result<Envelope, ProtocolError> {
    if frame.len() < 4 {
        return Err(ProtocolError::TruncatedPrefix {
            available: frame.len(),
        });
    }
    let header = FrameHeader::decode(&frame[..4])?;
    decode_payload(header, &frame[4..])
}

#[derive(Deserialize)]
struct VersionProbe {
    protocol_version: u16,
    #[serde(rename = "message")]
    _message: serde::de::IgnoredAny,
}

struct BoundedWriter {
    bytes: Vec<u8>,
    exceeded: bool,
    attempted_len: usize,
}

impl BoundedWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            exceeded: false,
            attempted_len: 0,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let attempted_len = self.bytes.len().saturating_add(buffer.len());
        if attempted_len > MAX_JSON_PAYLOAD_BYTES {
            self.exceeded = true;
            self.attempted_len = MAX_JSON_PAYLOAD_BYTES + 1;
            return Err(io::Error::other("protocol JSON payload limit exceeded"));
        }
        self.attempted_len = attempted_len;
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hector_core::{GenerationEpoch, RequestId};

    use super::{FrameHeader, MAX_JSON_PAYLOAD_BYTES, decode_frame, decode_payload, encode_frame};
    use crate::{
        Envelope, GenerationCorrelation, ProtocolError, ProtocolMessage, WorkPayload,
        WorkSubmission, WorkerRole,
    };

    fn correlation() -> GenerationCorrelation {
        GenerationCorrelation::new(
            GenerationEpoch::from_raw(3).expect("nonzero"),
            RequestId::from_raw(4).expect("nonzero"),
        )
    }

    fn cancel_envelope() -> Envelope {
        Envelope::new(ProtocolMessage::Cancel {
            correlation: correlation(),
        })
    }

    fn frame_payload(payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn valid_frame_round_trips_with_big_endian_payload_length() {
        let envelope = cancel_envelope();
        let frame = encode_frame(&envelope).expect("valid frame encodes");
        let declared = u32::from_be_bytes(frame[..4].try_into().expect("four bytes")) as usize;

        assert_eq!(declared, frame.len() - 4);
        assert_eq!(decode_frame(&frame), Ok(envelope));
        assert_eq!(
            encode_frame(&cancel_envelope()),
            encode_frame(&cancel_envelope())
        );
    }

    #[test]
    fn prefix_rejects_partial_zero_and_oversized_lengths() {
        for available in 0..4 {
            assert_eq!(
                FrameHeader::decode(&[0; 4][..available]),
                Err(ProtocolError::TruncatedPrefix { available })
            );
        }
        assert_eq!(
            FrameHeader::decode(&0_u32.to_be_bytes()),
            Err(ProtocolError::ZeroLengthFrame)
        );
        assert_eq!(
            FrameHeader::decode(&((MAX_JSON_PAYLOAD_BYTES + 1) as u32).to_be_bytes()),
            Err(ProtocolError::OversizedFrame {
                size: MAX_JSON_PAYLOAD_BYTES + 1,
            })
        );
    }

    #[test]
    fn payload_length_is_exact_and_one_frame_only() {
        let header = FrameHeader::decode(&5_u32.to_be_bytes()).expect("valid header");
        assert_eq!(header.payload_len(), 5);
        assert_eq!(
            decode_payload(header, b"1234"),
            Err(ProtocolError::TruncatedPayload {
                declared: 5,
                available: 4,
            })
        );
        assert_eq!(
            decode_payload(header, b"123456"),
            Err(ProtocolError::TrailingBytes { count: 1 })
        );
    }

    #[test]
    fn invalid_utf8_json_and_schema_are_distinct() {
        assert_eq!(
            decode_frame(&frame_payload(&[0xff])),
            Err(ProtocolError::InvalidUtf8)
        );
        assert_eq!(
            decode_frame(&frame_payload(b"{")),
            Err(ProtocolError::InvalidJson)
        );
        assert_eq!(
            decode_frame(&frame_payload(b"{}")),
            Err(ProtocolError::StructurallyInvalidMessage)
        );
    }

    #[test]
    fn unsupported_version_precedes_version_one_message_interpretation() {
        for version in [0, 2] {
            let json =
                format!(r#"{{"protocol_version":{version},"message":{{"type":"unknown"}}}}"#);
            assert_eq!(
                decode_frame(&frame_payload(json.as_bytes())),
                Err(ProtocolError::UnsupportedProtocolVersion { received: version })
            );
        }
    }

    #[test]
    fn exact_maximum_json_payload_is_accepted_and_one_more_is_rejected() {
        let prefix =
            br#"{"protocol_version":1,"message":{"type":"hello","payload":{"worker":"asr"}}}"#;
        let mut payload = Vec::with_capacity(MAX_JSON_PAYLOAD_BYTES);
        payload.extend_from_slice(prefix);
        payload.resize(MAX_JSON_PAYLOAD_BYTES, b' ');
        assert_eq!(
            decode_frame(&frame_payload(&payload))
                .expect("maximum payload is valid")
                .message(),
            &ProtocolMessage::Hello {
                worker: WorkerRole::Asr,
            }
        );

        assert_eq!(
            FrameHeader::decode(&((MAX_JSON_PAYLOAD_BYTES + 1) as u32).to_be_bytes()),
            Err(ProtocolError::OversizedFrame {
                size: MAX_JSON_PAYLOAD_BYTES + 1,
            })
        );
    }

    #[test]
    fn encoder_stops_at_the_first_byte_beyond_the_limit() {
        let oversized = Envelope::new(ProtocolMessage::Work(WorkSubmission::new(
            correlation(),
            WorkPayload::Llm {
                prompt: "x".repeat(MAX_JSON_PAYLOAD_BYTES),
            },
        )));

        assert_eq!(
            encode_frame(&oversized),
            Err(ProtocolError::OversizedFrame {
                size: MAX_JSON_PAYLOAD_BYTES + 1,
            })
        );
    }

    #[test]
    fn unsupported_version_does_not_use_version_one_unknown_field_rules() {
        let json = br#"{"protocol_version":2,"future_field":true,"message":{"type":"future"}}"#;
        assert_eq!(
            decode_frame(&frame_payload(json)),
            Err(ProtocolError::UnsupportedProtocolVersion { received: 2 })
        );
    }

    #[test]
    fn trailing_frame_bytes_are_rejected() {
        let mut frame = encode_frame(&cancel_envelope()).expect("valid frame");
        frame.push(0);
        assert_eq!(
            decode_frame(&frame),
            Err(ProtocolError::TrailingBytes { count: 1 })
        );
    }
}
