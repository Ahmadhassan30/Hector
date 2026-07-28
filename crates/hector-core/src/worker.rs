/// Kind of supervised worker or worker-like audio boundary.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WorkerKind {
    /// Automatic speech recognition.
    Asr,
    /// Language-model generation.
    Llm,
    /// Text-to-speech synthesis.
    Tts,
    /// Audio input or output.
    Audio,
}

/// Stable, dependency-free classification of a worker failure.
///
/// Detailed diagnostics remain infrastructure-owned and may be correlated
/// separately with a request identity.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WorkerFault {
    /// The worker or required resource is unavailable.
    Unavailable,
    /// The worker violated its communication protocol.
    Protocol,
    /// The worker returned a structurally invalid response.
    InvalidResponse,
    /// Work ended because it was cancelled.
    Cancelled,
    /// The worker process ended unexpectedly.
    UnexpectedTermination,
}

#[cfg(test)]
mod tests {
    use super::{WorkerFault, WorkerKind};

    #[test]
    fn worker_kinds_are_distinct_and_have_deterministic_debug_output() {
        let kinds = [
            WorkerKind::Asr,
            WorkerKind::Llm,
            WorkerKind::Tts,
            WorkerKind::Audio,
        ];

        for (index, left) in kinds.iter().enumerate() {
            for (other_index, right) in kinds.iter().enumerate() {
                assert_eq!(left == right, index == other_index);
            }
        }

        assert_eq!(format!("{:?}", WorkerKind::Asr), "Asr");
        assert_eq!(format!("{:?}", WorkerKind::Llm), "Llm");
        assert_eq!(format!("{:?}", WorkerKind::Tts), "Tts");
        assert_eq!(format!("{:?}", WorkerKind::Audio), "Audio");
    }

    #[test]
    fn worker_fault_classifications_are_distinct_and_stable() {
        let faults = [
            WorkerFault::Unavailable,
            WorkerFault::Protocol,
            WorkerFault::InvalidResponse,
            WorkerFault::Cancelled,
            WorkerFault::UnexpectedTermination,
        ];

        for (index, left) in faults.iter().enumerate() {
            for (other_index, right) in faults.iter().enumerate() {
                assert_eq!(left == right, index == other_index);
            }
        }

        assert_eq!(format!("{:?}", WorkerFault::Unavailable), "Unavailable");
        assert_eq!(format!("{:?}", WorkerFault::Protocol), "Protocol");
        assert_eq!(
            format!("{:?}", WorkerFault::InvalidResponse),
            "InvalidResponse"
        );
        assert_eq!(format!("{:?}", WorkerFault::Cancelled), "Cancelled");
        assert_eq!(
            format!("{:?}", WorkerFault::UnexpectedTermination),
            "UnexpectedTermination"
        );
    }
}
