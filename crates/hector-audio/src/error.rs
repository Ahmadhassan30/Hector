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
    use super::{AudioBufferError, PopError, PushError};
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
}
