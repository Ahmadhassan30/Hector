use crate::{
    AudioDeviceId, AudioDeviceInventory, AudioDiscoveryError, AudioDiscoveryLimit, AudioEndpoint,
    AudioFormatRange, MAX_AUDIO_DEVICE_ID_BYTES, MAX_AUDIO_DEVICE_NAME_BYTES, MAX_AUDIO_ENDPOINTS,
    MAX_FORMAT_RANGES_PER_DIRECTION,
};

#[derive(Clone, Debug)]
struct BackendEndpoint {
    id: String,
    name: String,
    input_formats: Vec<AudioFormatRange>,
    output_formats: Vec<AudioFormatRange>,
}

#[derive(Clone, Debug)]
struct BackendSnapshot {
    endpoints: Vec<BackendEndpoint>,
    default_input_id: Option<String>,
    default_output_id: Option<String>,
}

trait DiscoveryBackend {
    fn snapshot(&self) -> Result<BackendSnapshot, AudioDiscoveryError>;
}

/// Enumerates a complete point-in-time WASAPI inventory.
///
/// The returned values own no CPAL object, COM object, stream, callback, or
/// Windows handle.
pub fn enumerate_audio_devices() -> Result<AudioDeviceInventory, AudioDiscoveryError> {
    #[cfg(windows)]
    {
        enumerate_with_backend(&CpalWasapiBackend)
    }

    #[cfg(not(windows))]
    {
        Err(AudioDiscoveryError::UnsupportedPlatform)
    }
}

fn enumerate_with_backend(
    backend: &impl DiscoveryBackend,
) -> Result<AudioDeviceInventory, AudioDiscoveryError> {
    build_inventory(backend.snapshot()?)
}

fn build_inventory(snapshot: BackendSnapshot) -> Result<AudioDeviceInventory, AudioDiscoveryError> {
    if snapshot.endpoints.len() > MAX_AUDIO_ENDPOINTS {
        return Err(AudioDiscoveryError::LimitExceeded {
            limit: AudioDiscoveryLimit::EndpointCount,
            maximum: MAX_AUDIO_ENDPOINTS,
        });
    }

    let default_input_id = snapshot
        .default_input_id
        .as_deref()
        .map(parse_backend_id)
        .transpose()?;
    let default_output_id = snapshot
        .default_output_id
        .as_deref()
        .map(parse_backend_id)
        .transpose()?;
    let mut endpoints = Vec::with_capacity(snapshot.endpoints.len());

    for mut endpoint in snapshot.endpoints {
        if endpoint.name.len() > MAX_AUDIO_DEVICE_NAME_BYTES {
            return Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::DeviceNameBytes,
                maximum: MAX_AUDIO_DEVICE_NAME_BYTES,
            });
        }
        check_format_count(
            endpoint.input_formats.len(),
            AudioDiscoveryLimit::InputFormatCount,
        )?;
        check_format_count(
            endpoint.output_formats.len(),
            AudioDiscoveryLimit::OutputFormatCount,
        )?;
        endpoint.input_formats.sort_unstable();
        endpoint.input_formats.dedup();
        endpoint.output_formats.sort_unstable();
        endpoint.output_formats.dedup();
        endpoints.push(AudioEndpoint::new(
            parse_backend_id(&endpoint.id)?,
            endpoint.name,
            endpoint.input_formats,
            endpoint.output_formats,
        )?);
    }

    AudioDeviceInventory::new(endpoints, default_input_id, default_output_id)
}

fn parse_backend_id(value: &str) -> Result<AudioDeviceId, AudioDiscoveryError> {
    if value.len() > MAX_AUDIO_DEVICE_ID_BYTES {
        return Err(AudioDiscoveryError::LimitExceeded {
            limit: AudioDiscoveryLimit::DeviceIdBytes,
            maximum: MAX_AUDIO_DEVICE_ID_BYTES,
        });
    }
    AudioDeviceId::parse(value).map_err(AudioDiscoveryError::InvalidDeviceId)
}

fn check_format_count(count: usize, limit: AudioDiscoveryLimit) -> Result<(), AudioDiscoveryError> {
    if count > MAX_FORMAT_RANGES_PER_DIRECTION {
        Err(AudioDiscoveryError::LimitExceeded {
            limit,
            maximum: MAX_FORMAT_RANGES_PER_DIRECTION,
        })
    } else {
        Ok(())
    }
}

#[cfg(windows)]
struct CpalWasapiBackend;

#[cfg(windows)]
impl DiscoveryBackend for CpalWasapiBackend {
    fn snapshot(&self) -> Result<BackendSnapshot, AudioDiscoveryError> {
        use crate::{AudioDiscoveryOperation, MAX_AUDIO_DEVICE_ID_BYTES};
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = cpal::host_from_id(cpal::HostId::Wasapi)
            .map_err(|error| backend_error(AudioDiscoveryOperation::OpenWasapiHost, error))?;
        let default_input_id = default_device_id(
            host.default_input_device(),
            AudioDiscoveryOperation::ReadDefaultInput,
        )?;
        let default_output_id = default_device_id(
            host.default_output_device(),
            AudioDiscoveryOperation::ReadDefaultOutput,
        )?;
        let devices = host
            .devices()
            .map_err(|error| backend_error(AudioDiscoveryOperation::EnumerateDevices, error))?;
        let mut endpoints = Vec::new();

        for device in devices {
            if endpoints.len() == MAX_AUDIO_ENDPOINTS {
                return Err(AudioDiscoveryError::LimitExceeded {
                    limit: AudioDiscoveryLimit::EndpointCount,
                    maximum: MAX_AUDIO_ENDPOINTS,
                });
            }
            let id = device
                .id()
                .map_err(|error| backend_error(AudioDiscoveryOperation::ReadDeviceId, error))?;
            let id = id.to_string();
            if id.len() > MAX_AUDIO_DEVICE_ID_BYTES {
                return Err(AudioDiscoveryError::LimitExceeded {
                    limit: AudioDiscoveryLimit::DeviceIdBytes,
                    maximum: MAX_AUDIO_DEVICE_ID_BYTES,
                });
            }
            let description = device
                .description()
                .map_err(|error| backend_error(AudioDiscoveryOperation::ReadDescription, error))?;
            let name = description.name().to_owned();
            if name.len() > MAX_AUDIO_DEVICE_NAME_BYTES {
                return Err(AudioDiscoveryError::LimitExceeded {
                    limit: AudioDiscoveryLimit::DeviceNameBytes,
                    maximum: MAX_AUDIO_DEVICE_NAME_BYTES,
                });
            }

            let input_formats = if description.supports_input() {
                collect_formats(
                    device.supported_input_configs().map_err(|error| {
                        backend_error(AudioDiscoveryOperation::ReadInputFormats, error)
                    })?,
                    AudioDiscoveryLimit::InputFormatCount,
                )?
            } else {
                Vec::new()
            };
            let output_formats = if description.supports_output() {
                collect_formats(
                    device.supported_output_configs().map_err(|error| {
                        backend_error(AudioDiscoveryOperation::ReadOutputFormats, error)
                    })?,
                    AudioDiscoveryLimit::OutputFormatCount,
                )?
            } else {
                Vec::new()
            };

            endpoints.push(BackendEndpoint {
                id,
                name,
                input_formats,
                output_formats,
            });
        }

        Ok(BackendSnapshot {
            endpoints,
            default_input_id,
            default_output_id,
        })
    }
}

#[cfg(windows)]
fn default_device_id(
    device: Option<cpal::Device>,
    operation: crate::AudioDiscoveryOperation,
) -> Result<Option<String>, AudioDiscoveryError> {
    use cpal::traits::DeviceTrait;

    device
        .map(|device| {
            device
                .id()
                .map(|id| id.to_string())
                .map_err(|error| backend_error(operation, error))
        })
        .transpose()
}

#[cfg(windows)]
fn collect_formats(
    ranges: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
    limit: AudioDiscoveryLimit,
) -> Result<Vec<AudioFormatRange>, AudioDiscoveryError> {
    let mut formats = Vec::new();
    for range in ranges {
        if formats.len() == MAX_FORMAT_RANGES_PER_DIRECTION {
            return Err(AudioDiscoveryError::LimitExceeded {
                limit,
                maximum: MAX_FORMAT_RANGES_PER_DIRECTION,
            });
        }
        formats.push(map_format_range(range)?);
    }
    Ok(formats)
}

#[cfg(windows)]
fn map_format_range(
    range: cpal::SupportedStreamConfigRange,
) -> Result<AudioFormatRange, AudioDiscoveryError> {
    use crate::{AudioBufferCapability, AudioBufferRange};

    let buffer_capability = match range.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => AudioBufferCapability::Range(
            AudioBufferRange::new(*min, *max).map_err(AudioDiscoveryError::InvalidFormat)?,
        ),
        cpal::SupportedBufferSize::Unknown => AudioBufferCapability::Unknown,
    };
    AudioFormatRange::new(
        range.channels(),
        range.min_sample_rate(),
        range.max_sample_rate(),
        map_sample_format(range.sample_format()),
        buffer_capability,
    )
    .map_err(AudioDiscoveryError::InvalidFormat)
}

#[cfg(windows)]
fn map_sample_format(format: cpal::SampleFormat) -> crate::DeviceSampleFormat {
    use crate::DeviceSampleFormat as Hector;

    match format {
        cpal::SampleFormat::I8 => Hector::I8,
        cpal::SampleFormat::I16 => Hector::I16,
        cpal::SampleFormat::I24 => Hector::I24,
        cpal::SampleFormat::I32 => Hector::I32,
        cpal::SampleFormat::I64 => Hector::I64,
        cpal::SampleFormat::U8 => Hector::U8,
        cpal::SampleFormat::U16 => Hector::U16,
        cpal::SampleFormat::U24 => Hector::U24,
        cpal::SampleFormat::U32 => Hector::U32,
        cpal::SampleFormat::U64 => Hector::U64,
        cpal::SampleFormat::F32 => Hector::F32,
        cpal::SampleFormat::F64 => Hector::F64,
        cpal::SampleFormat::DsdU8 => Hector::DsdU8,
        cpal::SampleFormat::DsdU16 => Hector::DsdU16,
        cpal::SampleFormat::DsdU32 => Hector::DsdU32,
        _ => Hector::UnknownFuture,
    }
}

#[cfg(windows)]
fn backend_error(
    operation: crate::AudioDiscoveryOperation,
    error: cpal::Error,
) -> AudioDiscoveryError {
    AudioDiscoveryError::Backend {
        operation,
        kind: map_error_kind(error.kind()),
    }
}

#[cfg(windows)]
fn map_error_kind(kind: cpal::ErrorKind) -> crate::AudioBackendFailureKind {
    use crate::AudioBackendFailureKind as Hector;

    match kind {
        cpal::ErrorKind::DeviceBusy => Hector::DeviceBusy,
        cpal::ErrorKind::DeviceChanged => Hector::DeviceChanged,
        cpal::ErrorKind::DeviceNotAvailable => Hector::DeviceNotAvailable,
        cpal::ErrorKind::HostUnavailable => Hector::HostUnavailable,
        cpal::ErrorKind::InvalidInput => Hector::InvalidInput,
        cpal::ErrorKind::PermissionDenied => Hector::PermissionDenied,
        cpal::ErrorKind::RealtimeDenied => Hector::RealtimeDenied,
        cpal::ErrorKind::ResourceExhausted => Hector::ResourceExhausted,
        cpal::ErrorKind::StreamInvalidated => Hector::StreamInvalidated,
        cpal::ErrorKind::UnsupportedConfig => Hector::UnsupportedConfig,
        cpal::ErrorKind::UnsupportedOperation => Hector::UnsupportedOperation,
        cpal::ErrorKind::Xrun => Hector::Xrun,
        cpal::ErrorKind::BackendError => Hector::BackendError,
        cpal::ErrorKind::Other => Hector::Other,
        _ => Hector::UnknownFuture,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendEndpoint, BackendSnapshot, DiscoveryBackend, build_inventory, enumerate_with_backend,
    };
    use crate::{
        AudioBackendFailureKind, AudioBufferCapability, AudioDeviceIdError, AudioDiscoveryError,
        AudioDiscoveryLimit, AudioDiscoveryOperation, AudioFormatRange, DeviceSampleFormat,
        MAX_AUDIO_DEVICE_ID_BYTES, MAX_AUDIO_DEVICE_NAME_BYTES, MAX_AUDIO_ENDPOINTS,
        MAX_FORMAT_RANGES_PER_DIRECTION,
    };

    #[derive(Clone)]
    struct FakeBackend(Result<BackendSnapshot, AudioDiscoveryError>);

    impl DiscoveryBackend for FakeBackend {
        fn snapshot(&self) -> Result<BackendSnapshot, AudioDiscoveryError> {
            self.0.clone()
        }
    }

    fn format() -> AudioFormatRange {
        AudioFormatRange::new(
            1,
            44_100,
            48_000,
            DeviceSampleFormat::I16,
            AudioBufferCapability::Unknown,
        )
        .expect("valid format")
    }

    fn endpoint(id: String) -> BackendEndpoint {
        BackendEndpoint {
            id,
            name: "Test microphone".to_owned(),
            input_formats: vec![format()],
            output_formats: Vec::new(),
        }
    }

    #[test]
    fn synthetic_snapshot_sorts_deduplicates_and_marks_defaults() {
        let snapshot = BackendSnapshot {
            endpoints: vec![
                endpoint("wasapi:b".to_owned()),
                BackendEndpoint {
                    id: "wasapi:a".to_owned(),
                    name: "Test output".to_owned(),
                    input_formats: Vec::new(),
                    output_formats: vec![format(), format()],
                },
            ],
            default_input_id: Some("wasapi:b".to_owned()),
            default_output_id: Some("wasapi:a".to_owned()),
        };
        let inventory = enumerate_with_backend(&FakeBackend(Ok(snapshot))).expect("inventory");
        assert_eq!(inventory.endpoints()[0].id().as_str(), "wasapi:a");
        assert_eq!(inventory.endpoints()[0].output_formats().len(), 1);
        assert!(inventory.endpoints()[0].is_default_output());
        assert!(inventory.endpoints()[1].is_default_input());
    }

    #[test]
    fn backend_failure_is_returned_without_a_partial_snapshot() {
        let operations = [
            AudioDiscoveryOperation::OpenWasapiHost,
            AudioDiscoveryOperation::EnumerateDevices,
            AudioDiscoveryOperation::ReadDeviceId,
            AudioDiscoveryOperation::ReadDescription,
            AudioDiscoveryOperation::ReadInputFormats,
            AudioDiscoveryOperation::ReadOutputFormats,
            AudioDiscoveryOperation::ReadDefaultInput,
            AudioDiscoveryOperation::ReadDefaultOutput,
        ];
        for operation in operations {
            let failure = AudioDiscoveryError::Backend {
                operation,
                kind: AudioBackendFailureKind::HostUnavailable,
            };
            assert_eq!(
                enumerate_with_backend(&FakeBackend(Err(failure))),
                Err(failure)
            );
        }
    }

    #[test]
    fn every_inventory_bound_fails_without_truncation() {
        let endpoints = (0..=MAX_AUDIO_ENDPOINTS)
            .map(|index| endpoint(format!("wasapi:{index}")))
            .collect();
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints,
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::EndpointCount,
                maximum: MAX_AUDIO_ENDPOINTS,
            })
        );

        let mut too_many_formats = endpoint("wasapi:formats".to_owned());
        too_many_formats.input_formats = vec![format(); MAX_FORMAT_RANGES_PER_DIRECTION + 1];
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints: vec![too_many_formats],
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::InputFormatCount,
                maximum: MAX_FORMAT_RANGES_PER_DIRECTION,
            })
        );

        let too_many_output_formats = BackendEndpoint {
            id: "wasapi:output-formats".to_owned(),
            name: "Test output".to_owned(),
            input_formats: Vec::new(),
            output_formats: vec![format(); MAX_FORMAT_RANGES_PER_DIRECTION + 1],
        };
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints: vec![too_many_output_formats.clone()],
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::OutputFormatCount,
                maximum: MAX_FORMAT_RANGES_PER_DIRECTION,
            })
        );
        let mut long_name = endpoint("wasapi:name".to_owned());
        long_name.name = "x".repeat(MAX_AUDIO_DEVICE_NAME_BYTES + 1);
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints: vec![long_name],
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::DeviceNameBytes,
                maximum: MAX_AUDIO_DEVICE_NAME_BYTES,
            })
        );

        let long_id = endpoint(format!("wasapi:{}", "x".repeat(MAX_AUDIO_DEVICE_ID_BYTES)));
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints: vec![long_id],
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::DeviceIdBytes,
                maximum: MAX_AUDIO_DEVICE_ID_BYTES,
            })
        );
    }

    #[test]
    fn malformed_ids_and_directionless_endpoints_are_explicit() {
        let mut bad_id = endpoint("not-wasapi".to_owned());
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints: vec![bad_id.clone()],
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::InvalidDeviceId(
                AudioDeviceIdError::WrongHost
            ))
        );
        bad_id.id = "wasapi:directionless".to_owned();
        bad_id.input_formats.clear();
        assert_eq!(
            build_inventory(BackendSnapshot {
                endpoints: vec![bad_id],
                default_input_id: None,
                default_output_id: None,
            }),
            Err(AudioDiscoveryError::DirectionlessEndpoint)
        );
    }

    #[cfg(windows)]
    #[test]
    fn known_cpal_error_kinds_map_without_diagnostic_strings() {
        use super::map_error_kind;

        let cases = [
            (
                cpal::ErrorKind::DeviceBusy,
                AudioBackendFailureKind::DeviceBusy,
            ),
            (
                cpal::ErrorKind::DeviceChanged,
                AudioBackendFailureKind::DeviceChanged,
            ),
            (
                cpal::ErrorKind::DeviceNotAvailable,
                AudioBackendFailureKind::DeviceNotAvailable,
            ),
            (
                cpal::ErrorKind::HostUnavailable,
                AudioBackendFailureKind::HostUnavailable,
            ),
            (
                cpal::ErrorKind::InvalidInput,
                AudioBackendFailureKind::InvalidInput,
            ),
            (
                cpal::ErrorKind::PermissionDenied,
                AudioBackendFailureKind::PermissionDenied,
            ),
            (
                cpal::ErrorKind::RealtimeDenied,
                AudioBackendFailureKind::RealtimeDenied,
            ),
            (
                cpal::ErrorKind::ResourceExhausted,
                AudioBackendFailureKind::ResourceExhausted,
            ),
            (
                cpal::ErrorKind::StreamInvalidated,
                AudioBackendFailureKind::StreamInvalidated,
            ),
            (
                cpal::ErrorKind::UnsupportedConfig,
                AudioBackendFailureKind::UnsupportedConfig,
            ),
            (
                cpal::ErrorKind::UnsupportedOperation,
                AudioBackendFailureKind::UnsupportedOperation,
            ),
            (cpal::ErrorKind::Xrun, AudioBackendFailureKind::Xrun),
            (
                cpal::ErrorKind::BackendError,
                AudioBackendFailureKind::BackendError,
            ),
            (cpal::ErrorKind::Other, AudioBackendFailureKind::Other),
        ];
        for (cpal, expected) in cases {
            assert_eq!(map_error_kind(cpal), expected);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_snapshot_contains_only_sorted_valid_backend_values() {
        let inventory = super::enumerate_audio_devices().expect("WASAPI discovery succeeds");
        assert!(inventory.endpoints().len() <= MAX_AUDIO_ENDPOINTS);
        for pair in inventory.endpoints().windows(2) {
            assert!(pair[0].id() < pair[1].id());
        }
        for endpoint in inventory.endpoints() {
            assert!(endpoint.id().as_str().starts_with("wasapi:"));
            assert!(
                endpoint.input_formats().len() <= MAX_FORMAT_RANGES_PER_DIRECTION
                    && endpoint.output_formats().len() <= MAX_FORMAT_RANGES_PER_DIRECTION
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn repeated_windows_discovery_does_not_retain_hector_resources() {
        for _ in 0..8 {
            drop(super::enumerate_audio_devices().expect("WASAPI discovery succeeds"));
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires owner observation of the current Windows audio inventory"]
    fn manual_windows_audio_inventory() {
        use crate::{DefaultVoiceSelectionPolicy, select_audio_format};

        let inventory = super::enumerate_audio_devices().expect("WASAPI discovery succeeds");
        let policy = DefaultVoiceSelectionPolicy::new().into_policy();
        println!("default input: {:?}", inventory.default_input_id());
        println!("default output: {:?}", inventory.default_output_id());
        for endpoint in inventory.endpoints() {
            println!("{} | {}", endpoint.id().as_str(), endpoint.name());
            println!(
                "  input={} default={} selected={:?}",
                endpoint.input_formats().len(),
                endpoint.is_default_input(),
                select_audio_format(
                    crate::AudioDirection::Input,
                    endpoint.input_formats(),
                    &policy
                )
            );
            println!(
                "  output={} default={} selected={:?}",
                endpoint.output_formats().len(),
                endpoint.is_default_output(),
                select_audio_format(
                    crate::AudioDirection::Output,
                    endpoint.output_formats(),
                    &policy
                )
            );
        }
    }
}
