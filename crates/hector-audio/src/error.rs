use core::fmt;

/// Failure to construct a bounded audio sample buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioBufferError {
    /// A queue cannot have zero usable sample slots.
    ZeroCapacity,
    /// The requested capacity cannot be represented by the internal layout.
    CapacityOverflow,
    /// Storage reservation failed after its layout was validated.
    AllocationFailed,
}

impl fmt::Display for AudioBufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("audio capacity must be positive"),
            Self::CapacityOverflow => {
                formatter.write_str("audio capacity exceeds the platform allocation layout")
            }
            Self::AllocationFailed => formatter.write_str("audio storage allocation failed"),
        }
    }
}

impl std::error::Error for AudioBufferError {}

/// Failure to accept a WASAPI backend identifier.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioDeviceIdError {
    Empty,
    TooLong,
    WrongHost,
    MissingBackendIdentifier,
    ControlCharacter,
}

/// Failure to construct an audio format capability.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioFormatError {
    ZeroChannels,
    ZeroSampleRate,
    InvertedSampleRateRange,
    ZeroBufferFrames,
    InvertedBufferRange,
}

/// Failure to construct a deterministic format-selection policy.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioSelectionPolicyError {
    EmptySampleFormats,
    TooManyPreferences,
    ZeroSampleRate,
    ZeroChannelCount,
    DuplicateSampleRate,
    DuplicateSampleFormat,
    DuplicateChannelCount,
    UnsupportedSampleFormat,
}

/// Audio-backend operation that failed while building an inventory snapshot.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioDiscoveryOperation {
    OpenWasapiHost,
    EnumerateDevices,
    ReadDeviceId,
    ReadDescription,
    ReadInputFormats,
    ReadOutputFormats,
    ReadDefaultInput,
    ReadDefaultOutput,
}

/// Stable classification of a CPAL/backend failure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioBackendFailureKind {
    DeviceBusy,
    DeviceChanged,
    DeviceNotAvailable,
    HostUnavailable,
    InvalidInput,
    PermissionDenied,
    RealtimeDenied,
    ResourceExhausted,
    StreamInvalidated,
    UnsupportedConfig,
    UnsupportedOperation,
    Xrun,
    BackendError,
    Other,
    UnknownFuture,
}

/// Bounded inventory resource.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioDiscoveryLimit {
    EndpointCount,
    InputFormatCount,
    OutputFormatCount,
    DeviceIdBytes,
    DeviceNameBytes,
}

/// Failure to build a complete, internally consistent endpoint snapshot.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioDiscoveryError {
    UnsupportedPlatform,
    Backend {
        operation: AudioDiscoveryOperation,
        kind: AudioBackendFailureKind,
    },
    LimitExceeded {
        limit: AudioDiscoveryLimit,
        maximum: usize,
    },
    InvalidDeviceId(AudioDeviceIdError),
    InvalidFormat(AudioFormatError),
    DuplicateDeviceId,
    DirectionlessEndpoint,
}

/// Failure to select an explicit endpoint or compatible format.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AudioSelectionError {
    DeviceNotFound,
    DirectionUnavailable,
    NoSupportedFormat,
}

macro_rules! debug_display_and_error {
    ($($type:ty),+ $(,)?) => {
        $(
            impl fmt::Display for $type {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(formatter, "{self:?}")
                }
            }

            impl std::error::Error for $type {}
        )+
    };
}

debug_display_and_error!(
    AudioDeviceIdError,
    AudioFormatError,
    AudioSelectionPolicyError,
    AudioDiscoveryError,
    AudioSelectionError,
);

/// Normal result of attempting to push into a full audio queue.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PushError {
    /// The queue was full; the enclosed sample was not consumed.
    Full(i16),
}

impl PushError {
    /// Returns the sample rejected by the queue.
    pub const fn sample(self) -> i16 {
        match self {
            Self::Full(sample) => sample,
        }
    }

    /// Consumes the outcome and returns the rejected sample.
    pub const fn into_sample(self) -> i16 {
        self.sample()
    }
}

/// Normal result of attempting to pop from an empty audio queue.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PopError {
    /// No sample was available.
    Empty,
}

#[cfg(test)]
mod tests {
    use super::{
        AudioBackendFailureKind, AudioBufferError, AudioDeviceIdError, AudioDiscoveryError,
        AudioDiscoveryOperation, AudioFormatError, AudioSelectionError, AudioSelectionPolicyError,
        PopError, PushError,
    };
    use std::error::Error;

    #[test]
    fn construction_errors_are_deterministic_errors() {
        let cases = [
            (
                AudioBufferError::ZeroCapacity,
                "audio capacity must be positive",
            ),
            (
                AudioBufferError::CapacityOverflow,
                "audio capacity exceeds the platform allocation layout",
            ),
            (
                AudioBufferError::AllocationFailed,
                "audio storage allocation failed",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
            assert!(error.source().is_none());
            assert_eq!(error, error.clone());
        }
    }

    #[test]
    fn operational_outcomes_preserve_their_payloads() {
        let full = PushError::Full(-123);
        assert_eq!(full.sample(), -123);
        assert_eq!(full.into_sample(), -123);
        assert_eq!(PopError::Empty, PopError::Empty);
    }

    #[test]
    fn h20_errors_are_deterministic_and_source_free() {
        let errors: Vec<Box<dyn Error>> = vec![
            Box::new(AudioDeviceIdError::WrongHost),
            Box::new(AudioFormatError::ZeroChannels),
            Box::new(AudioSelectionPolicyError::DuplicateSampleRate),
            Box::new(AudioDiscoveryError::Backend {
                operation: AudioDiscoveryOperation::EnumerateDevices,
                kind: AudioBackendFailureKind::HostUnavailable,
            }),
            Box::new(AudioSelectionError::NoSupportedFormat),
        ];

        for error in errors {
            assert!(!error.to_string().is_empty());
            assert!(error.source().is_none());
        }
    }
}
