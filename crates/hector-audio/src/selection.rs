use crate::device::SelectedAudioEndpoint;
use crate::{
    AudioDeviceId, AudioDeviceInventory, AudioDirection, AudioFormatRange, AudioSelectionError,
    AudioSelectionPolicyError, DeviceSampleFormat, SelectedAudioFormat,
};
use std::{cmp::Reverse, collections::HashSet};

pub const MAX_SELECTION_PREFERENCES: usize = 16;

/// Behavior when none of the preferred sample rates is available.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SampleRateFallback {
    HighestSupported,
    Reject,
}

/// Behavior when none of the preferred channel counts is available.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChannelFallback {
    LowestSupported,
    Reject,
}

/// Checked, deterministic ranking inputs for audio format selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioSelectionPolicy {
    preferred_sample_rates: Vec<u32>,
    preferred_sample_formats: Vec<DeviceSampleFormat>,
    preferred_input_channels: Vec<u16>,
    preferred_output_channels: Vec<u16>,
    sample_rate_fallback: SampleRateFallback,
    channel_fallback: ChannelFallback,
}

impl AudioSelectionPolicy {
    #[allow(clippy::too_many_arguments)] // The four independent rankings and two fallbacks are the policy.
    pub fn new(
        preferred_sample_rates: Vec<u32>,
        preferred_sample_formats: Vec<DeviceSampleFormat>,
        preferred_input_channels: Vec<u16>,
        preferred_output_channels: Vec<u16>,
        sample_rate_fallback: SampleRateFallback,
        channel_fallback: ChannelFallback,
    ) -> Result<Self, AudioSelectionPolicyError> {
        check_bound(&preferred_sample_rates)?;
        check_bound(&preferred_sample_formats)?;
        check_bound(&preferred_input_channels)?;
        check_bound(&preferred_output_channels)?;
        if preferred_sample_formats.is_empty() {
            return Err(AudioSelectionPolicyError::EmptySampleFormats);
        }
        if preferred_sample_rates.contains(&0) {
            return Err(AudioSelectionPolicyError::ZeroSampleRate);
        }
        if preferred_input_channels.contains(&0) || preferred_output_channels.contains(&0) {
            return Err(AudioSelectionPolicyError::ZeroChannelCount);
        }
        if has_duplicates(&preferred_sample_rates) {
            return Err(AudioSelectionPolicyError::DuplicateSampleRate);
        }
        if has_duplicates(&preferred_sample_formats) {
            return Err(AudioSelectionPolicyError::DuplicateSampleFormat);
        }
        if has_duplicates(&preferred_input_channels) || has_duplicates(&preferred_output_channels) {
            return Err(AudioSelectionPolicyError::DuplicateChannelCount);
        }
        if preferred_sample_formats
            .iter()
            .any(|format| !format.is_selectable())
        {
            return Err(AudioSelectionPolicyError::UnsupportedSampleFormat);
        }

        Ok(Self {
            preferred_sample_rates,
            preferred_sample_formats,
            preferred_input_channels,
            preferred_output_channels,
            sample_rate_fallback,
            channel_fallback,
        })
    }

    pub fn preferred_sample_rates(&self) -> &[u32] {
        &self.preferred_sample_rates
    }

    pub fn preferred_sample_formats(&self) -> &[DeviceSampleFormat] {
        &self.preferred_sample_formats
    }

    pub fn preferred_input_channels(&self) -> &[u16] {
        &self.preferred_input_channels
    }

    pub fn preferred_output_channels(&self) -> &[u16] {
        &self.preferred_output_channels
    }

    pub const fn sample_rate_fallback(&self) -> SampleRateFallback {
        self.sample_rate_fallback
    }

    pub const fn channel_fallback(&self) -> ChannelFallback {
        self.channel_fallback
    }

    fn preferred_channels(&self, direction: AudioDirection) -> &[u16] {
        match direction {
            AudioDirection::Input => &self.preferred_input_channels,
            AudioDirection::Output => &self.preferred_output_channels,
        }
    }
}

fn check_bound<T>(values: &[T]) -> Result<(), AudioSelectionPolicyError> {
    if values.len() > MAX_SELECTION_PREFERENCES {
        Err(AudioSelectionPolicyError::TooManyPreferences)
    } else {
        Ok(())
    }
}

fn has_duplicates<T: Copy + Eq + std::hash::Hash>(values: &[T]) -> bool {
    let mut seen = HashSet::with_capacity(values.len());
    values.iter().copied().any(|value| !seen.insert(value))
}

/// Hector's current voice-oriented format preferences.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DefaultVoiceSelectionPolicy;

#[allow(clippy::new_without_default)] // The named policy avoids implying that all selection is default-driven.
impl DefaultVoiceSelectionPolicy {
    pub const fn new() -> Self {
        Self
    }

    pub fn into_policy(self) -> AudioSelectionPolicy {
        AudioSelectionPolicy {
            preferred_sample_rates: vec![48_000, 44_100],
            preferred_sample_formats: vec![
                DeviceSampleFormat::I16,
                DeviceSampleFormat::F32,
                DeviceSampleFormat::U16,
            ],
            preferred_input_channels: vec![1],
            preferred_output_channels: vec![2, 1],
            sample_rate_fallback: SampleRateFallback::HighestSupported,
            channel_fallback: ChannelFallback::LowestSupported,
        }
    }
}

/// Selects one exact format without consulting device defaults or enumeration order.
pub fn select_audio_format(
    direction: AudioDirection,
    formats: &[AudioFormatRange],
    policy: &AudioSelectionPolicy,
) -> Result<SelectedAudioFormat, AudioSelectionError> {
    let mut best: Option<(CandidateKey, SelectedAudioFormat)> = None;

    for range in formats {
        let Some(format_rank) = policy
            .preferred_sample_formats
            .iter()
            .position(|format| *format == range.sample_format())
        else {
            continue;
        };
        let Some((rate_rank, sample_rate)) = choose_rate(*range, policy) else {
            continue;
        };
        let Some(channel_rank) = choose_channel_rank(range.channels(), direction, policy) else {
            continue;
        };

        let selected = SelectedAudioFormat::new(
            range.channels(),
            sample_rate,
            range.sample_format(),
            range.buffer_capability(),
        );
        let key = (
            rate_rank,
            format_rank,
            channel_rank,
            Reverse(sample_rate),
            range.channels(),
            range.minimum_sample_rate(),
            range.maximum_sample_rate(),
            range.buffer_capability(),
        );
        if best.as_ref().is_none_or(|(best_key, _)| key < *best_key) {
            best = Some((key, selected));
        }
    }

    best.map(|(_, selected)| selected)
        .ok_or(AudioSelectionError::NoSupportedFormat)
}

type CandidateKey = (
    usize,
    usize,
    usize,
    Reverse<u32>,
    u16,
    u32,
    u32,
    crate::AudioBufferCapability,
);

fn choose_rate(range: AudioFormatRange, policy: &AudioSelectionPolicy) -> Option<(usize, u32)> {
    if let Some((rank, rate)) = policy
        .preferred_sample_rates
        .iter()
        .copied()
        .enumerate()
        .find(|(_, rate)| range.contains_sample_rate(*rate))
    {
        return Some((rank, rate));
    }
    match policy.sample_rate_fallback {
        SampleRateFallback::HighestSupported => Some((
            policy.preferred_sample_rates.len(),
            range.maximum_sample_rate(),
        )),
        SampleRateFallback::Reject => None,
    }
}

fn choose_channel_rank(
    channels: u16,
    direction: AudioDirection,
    policy: &AudioSelectionPolicy,
) -> Option<usize> {
    let preferences = policy.preferred_channels(direction);
    if let Some(rank) = preferences
        .iter()
        .position(|candidate| *candidate == channels)
    {
        return Some(rank);
    }
    match policy.channel_fallback {
        ChannelFallback::LowestSupported => Some(preferences.len()),
        ChannelFallback::Reject => None,
    }
}

/// Selects a direction and format only from the explicitly named endpoint.
pub fn select_audio_endpoint(
    inventory: &AudioDeviceInventory,
    device_id: &AudioDeviceId,
    direction: AudioDirection,
    policy: &AudioSelectionPolicy,
) -> Result<SelectedAudioEndpoint, AudioSelectionError> {
    let endpoint = inventory
        .endpoint(device_id)
        .ok_or(AudioSelectionError::DeviceNotFound)?;
    let formats = match direction {
        AudioDirection::Input => endpoint.input_formats(),
        AudioDirection::Output => endpoint.output_formats(),
    };
    if formats.is_empty() {
        return Err(AudioSelectionError::DirectionUnavailable);
    }
    let selected = select_audio_format(direction, formats, policy)?;
    Ok(SelectedAudioEndpoint::new(
        device_id.clone(),
        direction,
        selected,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        AudioSelectionPolicy, ChannelFallback, DefaultVoiceSelectionPolicy,
        MAX_SELECTION_PREFERENCES, SampleRateFallback, select_audio_endpoint, select_audio_format,
    };
    use crate::{
        AudioBufferCapability, AudioDeviceId, AudioDeviceInventory, AudioDirection, AudioEndpoint,
        AudioFormatRange, AudioSelectionError, AudioSelectionPolicyError, DeviceSampleFormat,
    };

    fn range(
        channels: u16,
        minimum: u32,
        maximum: u32,
        format: DeviceSampleFormat,
    ) -> AudioFormatRange {
        AudioFormatRange::new(
            channels,
            minimum,
            maximum,
            format,
            AudioBufferCapability::Unknown,
        )
        .expect("valid range")
    }

    #[test]
    fn policy_validation_is_exact() {
        assert_eq!(
            AudioSelectionPolicy::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::EmptySampleFormats)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![0],
                vec![DeviceSampleFormat::I16],
                vec![1],
                vec![2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::ZeroSampleRate)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![48_000, 48_000],
                vec![DeviceSampleFormat::I16],
                vec![1],
                vec![2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::DuplicateSampleRate)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![48_000],
                vec![DeviceSampleFormat::F64],
                vec![1],
                vec![2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::UnsupportedSampleFormat)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![1; MAX_SELECTION_PREFERENCES + 1],
                vec![DeviceSampleFormat::I16],
                vec![1],
                vec![2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::TooManyPreferences)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![48_000],
                vec![DeviceSampleFormat::I16, DeviceSampleFormat::I16],
                vec![1],
                vec![2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::DuplicateSampleFormat)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![48_000],
                vec![DeviceSampleFormat::I16],
                vec![0],
                vec![2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::ZeroChannelCount)
        );
        assert_eq!(
            AudioSelectionPolicy::new(
                vec![48_000],
                vec![DeviceSampleFormat::I16],
                vec![1],
                vec![2, 2],
                SampleRateFallback::Reject,
                ChannelFallback::Reject
            ),
            Err(AudioSelectionPolicyError::DuplicateChannelCount)
        );
    }

    #[test]
    fn default_policy_is_data_not_embedded_selection_logic() {
        let policy = DefaultVoiceSelectionPolicy::new().into_policy();
        assert_eq!(policy.preferred_sample_rates(), &[48_000, 44_100]);
        assert_eq!(
            policy.preferred_sample_formats(),
            &[
                DeviceSampleFormat::I16,
                DeviceSampleFormat::F32,
                DeviceSampleFormat::U16
            ]
        );
        assert_eq!(policy.preferred_input_channels(), &[1]);
        assert_eq!(policy.preferred_output_channels(), &[2, 1]);
        assert_eq!(
            policy.sample_rate_fallback(),
            SampleRateFallback::HighestSupported
        );
        assert_eq!(policy.channel_fallback(), ChannelFallback::LowestSupported);
    }

    #[test]
    fn default_policy_prefers_rate_then_format_then_direction_channels() {
        let policy = DefaultVoiceSelectionPolicy::new().into_policy();
        let formats = [
            range(1, 44_100, 44_100, DeviceSampleFormat::I16),
            range(2, 48_000, 48_000, DeviceSampleFormat::F32),
            range(1, 48_000, 48_000, DeviceSampleFormat::I16),
        ];

        let input =
            select_audio_format(AudioDirection::Input, &formats, &policy).expect("selection");
        assert_eq!(input.sample_rate(), 48_000);
        assert_eq!(input.sample_format(), DeviceSampleFormat::I16);
        assert_eq!(input.channels(), 1);
    }

    #[test]
    fn custom_policy_replaces_default_ranking() {
        let policy = AudioSelectionPolicy::new(
            vec![44_100],
            vec![DeviceSampleFormat::F32, DeviceSampleFormat::I16],
            vec![2],
            vec![1],
            SampleRateFallback::Reject,
            ChannelFallback::Reject,
        )
        .expect("valid policy");
        let formats = [
            range(1, 48_000, 48_000, DeviceSampleFormat::I16),
            range(2, 44_100, 44_100, DeviceSampleFormat::F32),
        ];
        let selected =
            select_audio_format(AudioDirection::Input, &formats, &policy).expect("selection");
        assert_eq!(selected.sample_rate(), 44_100);
        assert_eq!(selected.sample_format(), DeviceSampleFormat::F32);
        assert_eq!(selected.channels(), 2);
    }

    #[test]
    fn fallbacks_are_deterministic_or_reject() {
        let formats = [
            range(6, 32_000, 96_000, DeviceSampleFormat::I16),
            range(4, 32_000, 88_200, DeviceSampleFormat::I16),
        ];
        let default = DefaultVoiceSelectionPolicy::new().into_policy();
        let selected =
            select_audio_format(AudioDirection::Input, &formats, &default).expect("fallback");
        assert_eq!(selected.sample_rate(), 48_000);
        assert_eq!(selected.channels(), 4);

        let reject = AudioSelectionPolicy::new(
            vec![192_000],
            vec![DeviceSampleFormat::I16],
            vec![1],
            vec![2],
            SampleRateFallback::Reject,
            ChannelFallback::Reject,
        )
        .expect("valid policy");
        assert_eq!(
            select_audio_format(AudioDirection::Input, &formats, &reject),
            Err(AudioSelectionError::NoSupportedFormat)
        );

        let no_preferred_rate = [range(1, 32_000, 96_000, DeviceSampleFormat::I16)];
        let selected = select_audio_format(AudioDirection::Input, &no_preferred_rate, &default)
            .expect("highest-rate fallback");
        assert_eq!(selected.sample_rate(), 48_000);

        let only_fallback_rate = [range(1, 32_000, 40_000, DeviceSampleFormat::I16)];
        let selected = select_audio_format(AudioDirection::Input, &only_fallback_rate, &default)
            .expect("highest-rate fallback");
        assert_eq!(selected.sample_rate(), 40_000);
    }

    #[test]
    fn unsupported_formats_remain_explicit_but_unselectable() {
        let formats = [range(1, 48_000, 48_000, DeviceSampleFormat::F64)];
        assert_eq!(
            select_audio_format(
                AudioDirection::Input,
                &formats,
                &DefaultVoiceSelectionPolicy::new().into_policy()
            ),
            Err(AudioSelectionError::NoSupportedFormat)
        );
    }

    #[test]
    fn enumeration_order_never_breaks_ties() {
        let first = range(1, 44_100, 96_000, DeviceSampleFormat::I16);
        let second = range(1, 48_000, 48_000, DeviceSampleFormat::I16);
        let policy = DefaultVoiceSelectionPolicy::new().into_policy();
        let forward =
            select_audio_format(AudioDirection::Input, &[first, second], &policy).expect("select");
        let reverse =
            select_audio_format(AudioDirection::Input, &[second, first], &policy).expect("select");
        assert_eq!(forward, reverse);
    }

    #[test]
    fn endpoint_selection_requires_exact_id_direction_and_compatible_format() {
        let input_id = AudioDeviceId::parse("wasapi:input").expect("valid ID");
        let output_id = AudioDeviceId::parse("wasapi:output").expect("valid ID");
        let inventory = AudioDeviceInventory::new(
            vec![
                AudioEndpoint::new(
                    input_id.clone(),
                    "Input".to_owned(),
                    vec![range(1, 48_000, 48_000, DeviceSampleFormat::I16)],
                    Vec::new(),
                )
                .expect("valid endpoint"),
                AudioEndpoint::new(
                    output_id.clone(),
                    "Output".to_owned(),
                    Vec::new(),
                    vec![range(2, 48_000, 48_000, DeviceSampleFormat::F64)],
                )
                .expect("valid endpoint"),
            ],
            None,
            None,
        )
        .expect("valid inventory");
        let policy = DefaultVoiceSelectionPolicy::new().into_policy();

        let selected = select_audio_endpoint(&inventory, &input_id, AudioDirection::Input, &policy)
            .expect("selected input");
        assert_eq!(selected.device_id(), &input_id);
        assert_eq!(selected.direction(), AudioDirection::Input);
        assert_eq!(selected.format().sample_rate(), 48_000);
        assert_eq!(
            select_audio_endpoint(&inventory, &input_id, AudioDirection::Output, &policy),
            Err(AudioSelectionError::DirectionUnavailable)
        );
        assert_eq!(
            select_audio_endpoint(&inventory, &output_id, AudioDirection::Output, &policy),
            Err(AudioSelectionError::NoSupportedFormat)
        );
        assert_eq!(
            select_audio_endpoint(
                &inventory,
                &AudioDeviceId::parse("wasapi:missing").expect("valid ID"),
                AudioDirection::Input,
                &policy
            ),
            Err(AudioSelectionError::DeviceNotFound)
        );
    }
}
