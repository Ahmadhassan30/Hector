use crate::{
    AudioDeviceIdError, AudioDiscoveryError, AudioDiscoveryLimit, AudioFormatRange,
    SelectedAudioFormat,
};

pub const MAX_AUDIO_ENDPOINTS: usize = 256;
pub const MAX_FORMAT_RANGES_PER_DIRECTION: usize = 256;
pub const MAX_AUDIO_DEVICE_ID_BYTES: usize = 4096;
pub const MAX_AUDIO_DEVICE_NAME_BYTES: usize = 1024;

const WASAPI_PREFIX: &str = "wasapi:";

/// Direction in which an endpoint carries audio.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AudioDirection {
    Input,
    Output,
}

/// Opaque CPAL/WASAPI backend identifier for the current platform snapshot.
///
/// Hector does not promise that this value survives hardware, driver, Windows,
/// or backend upgrades. A previously recorded value is usable only when it
/// appears in a newly enumerated inventory.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioDeviceId(String);

impl AudioDeviceId {
    pub fn parse(value: &str) -> Result<Self, AudioDeviceIdError> {
        if value.is_empty() {
            return Err(AudioDeviceIdError::Empty);
        }
        if value.len() > MAX_AUDIO_DEVICE_ID_BYTES {
            return Err(AudioDeviceIdError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(AudioDeviceIdError::ControlCharacter);
        }
        let Some(identifier) = value.strip_prefix(WASAPI_PREFIX) else {
            return Err(AudioDeviceIdError::WrongHost);
        };
        if identifier.is_empty() {
            return Err(AudioDeviceIdError::MissingBackendIdentifier);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

/// One active endpoint and its point-in-time advertised capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioEndpoint {
    id: AudioDeviceId,
    name: String,
    is_default_input: bool,
    is_default_output: bool,
    input_formats: Vec<AudioFormatRange>,
    output_formats: Vec<AudioFormatRange>,
}

impl AudioEndpoint {
    pub(crate) fn new(
        id: AudioDeviceId,
        name: String,
        input_formats: Vec<AudioFormatRange>,
        output_formats: Vec<AudioFormatRange>,
    ) -> Result<Self, AudioDiscoveryError> {
        if name.len() > MAX_AUDIO_DEVICE_NAME_BYTES {
            return Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::DeviceNameBytes,
                maximum: MAX_AUDIO_DEVICE_NAME_BYTES,
            });
        }
        if input_formats.is_empty() && output_formats.is_empty() {
            return Err(AudioDiscoveryError::DirectionlessEndpoint);
        }
        Ok(Self {
            id,
            name,
            is_default_input: false,
            is_default_output: false,
            input_formats,
            output_formats,
        })
    }

    pub fn id(&self) -> &AudioDeviceId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn is_default_input(&self) -> bool {
        self.is_default_input
    }

    pub const fn is_default_output(&self) -> bool {
        self.is_default_output
    }

    pub fn input_formats(&self) -> &[AudioFormatRange] {
        &self.input_formats
    }

    pub fn output_formats(&self) -> &[AudioFormatRange] {
        &self.output_formats
    }

    pub(crate) fn set_default_input(&mut self, value: bool) {
        self.is_default_input = value;
    }

    pub(crate) fn set_default_output(&mut self, value: bool) {
        self.is_default_output = value;
    }
}

/// Immutable point-in-time WASAPI endpoint inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioDeviceInventory {
    endpoints: Vec<AudioEndpoint>,
    default_input_id: Option<AudioDeviceId>,
    default_output_id: Option<AudioDeviceId>,
}

impl AudioDeviceInventory {
    pub(crate) fn new(
        mut endpoints: Vec<AudioEndpoint>,
        default_input_id: Option<AudioDeviceId>,
        default_output_id: Option<AudioDeviceId>,
    ) -> Result<Self, AudioDiscoveryError> {
        if endpoints.len() > MAX_AUDIO_ENDPOINTS {
            return Err(AudioDiscoveryError::LimitExceeded {
                limit: AudioDiscoveryLimit::EndpointCount,
                maximum: MAX_AUDIO_ENDPOINTS,
            });
        }
        endpoints.sort_by(|left, right| left.id.cmp(&right.id));
        if endpoints.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(AudioDiscoveryError::DuplicateDeviceId);
        }
        for endpoint in &mut endpoints {
            endpoint.set_default_input(default_input_id.as_ref() == Some(endpoint.id()));
            endpoint.set_default_output(default_output_id.as_ref() == Some(endpoint.id()));
        }
        Ok(Self {
            endpoints,
            default_input_id,
            default_output_id,
        })
    }

    pub fn endpoints(&self) -> &[AudioEndpoint] {
        &self.endpoints
    }

    pub fn default_input_id(&self) -> Option<&AudioDeviceId> {
        self.default_input_id.as_ref()
    }

    pub fn default_output_id(&self) -> Option<&AudioDeviceId> {
        self.default_output_id.as_ref()
    }

    pub fn endpoint(&self, id: &AudioDeviceId) -> Option<&AudioEndpoint> {
        self.endpoints
            .binary_search_by(|endpoint| endpoint.id.cmp(id))
            .ok()
            .map(|index| &self.endpoints[index])
    }
}

/// Explicit endpoint and exact format selected from one inventory snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedAudioEndpoint {
    device_id: AudioDeviceId,
    direction: AudioDirection,
    format: SelectedAudioFormat,
}

impl SelectedAudioEndpoint {
    pub(crate) const fn new(
        device_id: AudioDeviceId,
        direction: AudioDirection,
        format: SelectedAudioFormat,
    ) -> Self {
        Self {
            device_id,
            direction,
            format,
        }
    }

    pub const fn device_id(&self) -> &AudioDeviceId {
        &self.device_id
    }

    pub const fn direction(&self) -> AudioDirection {
        self.direction
    }

    pub const fn format(&self) -> SelectedAudioFormat {
        self.format
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioDeviceId, AudioDeviceInventory, AudioDirection, AudioEndpoint,
        MAX_AUDIO_DEVICE_ID_BYTES,
    };
    use crate::{
        AudioBufferCapability, AudioDeviceIdError, AudioDiscoveryError, AudioFormatRange,
        DeviceSampleFormat,
    };

    fn format() -> AudioFormatRange {
        AudioFormatRange::new(
            1,
            48_000,
            48_000,
            DeviceSampleFormat::I16,
            AudioBufferCapability::Unknown,
        )
        .expect("valid format")
    }

    fn endpoint(id: &str) -> AudioEndpoint {
        AudioEndpoint::new(
            AudioDeviceId::parse(id).expect("valid ID"),
            id.to_owned(),
            vec![format()],
            Vec::new(),
        )
        .expect("valid endpoint")
    }

    #[test]
    fn identifiers_are_bounded_current_backend_keys() {
        assert_eq!(AudioDeviceId::parse(""), Err(AudioDeviceIdError::Empty));
        assert_eq!(
            AudioDeviceId::parse("asio:device"),
            Err(AudioDeviceIdError::WrongHost)
        );
        assert_eq!(
            AudioDeviceId::parse("wasapi:"),
            Err(AudioDeviceIdError::MissingBackendIdentifier)
        );
        assert_eq!(
            AudioDeviceId::parse("wasapi:bad\nid"),
            Err(AudioDeviceIdError::ControlCharacter)
        );
        let too_long = format!("wasapi:{}", "x".repeat(MAX_AUDIO_DEVICE_ID_BYTES));
        assert_eq!(
            AudioDeviceId::parse(&too_long),
            Err(AudioDeviceIdError::TooLong)
        );

        let unicode = AudioDeviceId::parse("wasapi:ヘッドセット").expect("valid Unicode");
        assert_eq!(unicode.as_str(), "wasapi:ヘッドセット");
        assert_eq!(unicode.clone().into_string(), unicode.as_str());
    }

    #[test]
    fn inventory_sorts_marks_defaults_and_rejects_duplicates() {
        let default = AudioDeviceId::parse("wasapi:b").expect("valid ID");
        let inventory = AudioDeviceInventory::new(
            vec![endpoint("wasapi:b"), endpoint("wasapi:a")],
            Some(default.clone()),
            None,
        )
        .expect("valid inventory");

        assert_eq!(inventory.endpoints()[0].id().as_str(), "wasapi:a");
        assert_eq!(inventory.endpoints()[1].id().as_str(), "wasapi:b");
        assert!(
            inventory
                .endpoint(&default)
                .expect("endpoint")
                .is_default_input()
        );
        assert_eq!(inventory.default_input_id(), Some(&default));
        assert_eq!(inventory.default_output_id(), None);

        assert_eq!(
            AudioDeviceInventory::new(vec![endpoint("wasapi:a"), endpoint("wasapi:a")], None, None),
            Err(AudioDiscoveryError::DuplicateDeviceId)
        );
    }

    #[test]
    fn endpoint_direction_is_explicit() {
        let endpoint = endpoint("wasapi:input");
        assert!(!endpoint.input_formats().is_empty());
        assert!(endpoint.output_formats().is_empty());
        assert_eq!(AudioDirection::Input, AudioDirection::Input);
    }

    #[test]
    fn inventory_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AudioDeviceId>();
        assert_send_sync::<AudioDeviceInventory>();
        assert_send_sync::<AudioEndpoint>();
    }
}
