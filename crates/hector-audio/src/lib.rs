//! Callback-safe audio storage, observations, and device discovery for Hector.
//!
//! This crate owns bounded native-sample primitives, point-in-time WASAPI
//! endpoint inventory through CPAL, and deterministic format selection. Device
//! streams, protocol conversion, resampling, recovery policy, and reducer
//! decisions remain outside this boundary.

#[cfg(not(target_has_atomic = "64"))]
compile_error!("hector-audio requires lock-free 64-bit atomics");

mod control;
mod counters;
mod device;
mod discovery;
mod error;
mod format;
mod selection;
mod spsc;

pub use control::{AudioEpochSlot, UrgentGenerationSlot};
pub use counters::{AudioCounterReader, AudioCounterSnapshot};
pub use device::{
    AudioDeviceId, AudioDeviceInventory, AudioDirection, AudioEndpoint, MAX_AUDIO_DEVICE_ID_BYTES,
    MAX_AUDIO_DEVICE_NAME_BYTES, MAX_AUDIO_ENDPOINTS, MAX_FORMAT_RANGES_PER_DIRECTION,
    SelectedAudioEndpoint,
};
pub use discovery::enumerate_audio_devices;
pub use error::{
    AudioBackendFailureKind, AudioBufferError, AudioDeviceIdError, AudioDiscoveryError,
    AudioDiscoveryLimit, AudioDiscoveryOperation, AudioFormatError, AudioSelectionError,
    AudioSelectionPolicyError, PopError, PushError,
};
pub use format::{
    AudioBufferCapability, AudioBufferRange, AudioFormatRange, DeviceSampleFormat,
    SelectedAudioFormat,
};
pub use selection::{
    AudioSelectionPolicy, ChannelFallback, DefaultVoiceSelectionPolicy, MAX_SELECTION_PREFERENCES,
    SampleRateFallback, select_audio_endpoint, select_audio_format,
};
pub use spsc::{AudioConsumer, AudioProducer, AudioSpscBuffer};
