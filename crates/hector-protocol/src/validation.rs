use hector_core::{GenerationEpoch, RequestId};

use crate::{GenerationCorrelation, ProtocolError};

/// Validates assistant-output freshness using only `GenerationEpoch`.
pub fn validate_generation(
    active: GenerationEpoch,
    incoming: GenerationEpoch,
) -> Result<(), ProtocolError> {
    if incoming < active {
        return Err(ProtocolError::StaleGeneration { active, incoming });
    }
    if incoming > active {
        return Err(ProtocolError::UnexpectedFutureGeneration { active, incoming });
    }

    Ok(())
}

/// Validates generation freshness before exact request correlation.
pub fn validate_correlation(
    active_generation: GenerationEpoch,
    expected_request: RequestId,
    incoming: GenerationCorrelation,
) -> Result<(), ProtocolError> {
    validate_generation(active_generation, incoming.generation_epoch())?;
    if incoming.request_id() != expected_request {
        return Err(ProtocolError::RequestCorrelationMismatch {
            expected: expected_request,
            incoming: incoming.request_id(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use hector_core::{GenerationEpoch, RequestId};

    use super::{validate_correlation, validate_generation};
    use crate::{GenerationCorrelation, ProtocolError};

    fn generation(raw: u64) -> GenerationEpoch {
        GenerationEpoch::from_raw(raw).expect("test generations are nonzero")
    }

    fn request(raw: u128) -> RequestId {
        RequestId::from_raw(raw).expect("test requests are nonzero")
    }

    #[test]
    fn generation_validation_classifies_current_stale_and_future() {
        assert_eq!(validate_generation(generation(5), generation(5)), Ok(()));
        assert_eq!(
            validate_generation(generation(5), generation(4)),
            Err(ProtocolError::StaleGeneration {
                active: generation(5),
                incoming: generation(4),
            })
        );
        assert_eq!(
            validate_generation(generation(5), generation(6)),
            Err(ProtocolError::UnexpectedFutureGeneration {
                active: generation(5),
                incoming: generation(6),
            })
        );
    }

    #[test]
    fn request_is_checked_only_after_generation_is_current() {
        assert_eq!(
            validate_correlation(
                generation(5),
                request(7),
                GenerationCorrelation::new(generation(5), request(8)),
            ),
            Err(ProtocolError::RequestCorrelationMismatch {
                expected: request(7),
                incoming: request(8),
            })
        );

        for incoming_request in [request(7), request(8)] {
            assert_eq!(
                validate_correlation(
                    generation(5),
                    request(7),
                    GenerationCorrelation::new(generation(4), incoming_request),
                ),
                Err(ProtocolError::StaleGeneration {
                    active: generation(5),
                    incoming: generation(4),
                })
            );
        }
    }
}
