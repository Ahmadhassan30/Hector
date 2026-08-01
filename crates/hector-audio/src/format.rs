use crate::AudioFormatError;

/// Sample representation reported by the active audio backend.
///
/// H20 can select only [`Self::I16`], [`Self::F32`], and [`Self::U16`].
/// Other variants remain visible so unsupported hardware is explicit.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceSampleFormat {
    I8,
    I16,
    I24,
    I32,
    I64,
    U8,
    U16,
    U24,
    U32,
    U64,
    F32,
    F64,
    DsdU8,
    DsdU16,
    DsdU32,
    UnknownFuture,
}

impl DeviceSampleFormat {
    /// Whether H21/H22 may perform a fixed callback conversion for this format.
    pub const fn is_selectable(self) -> bool {
        matches!(self, Self::I16 | Self::F32 | Self::U16)
    }
}

/// Supported callback-buffer-size information.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AudioBufferCapability {
    Unknown,
    Range(AudioBufferRange),
}

/// Inclusive device callback-buffer-size range measured in frames.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioBufferRange {
    minimum_frames: u32,
    maximum_frames: u32,
}

impl AudioBufferRange {
    pub fn new(minimum_frames: u32, maximum_frames: u32) -> Result<Self, AudioFormatError> {
        if minimum_frames == 0 || maximum_frames == 0 {
            return Err(AudioFormatError::ZeroBufferFrames);
        }
        if minimum_frames > maximum_frames {
            return Err(AudioFormatError::InvertedBufferRange);
        }
        Ok(Self {
            minimum_frames,
            maximum_frames,
        })
    }

    pub const fn minimum_frames(self) -> u32 {
        self.minimum_frames
    }

    pub const fn maximum_frames(self) -> u32 {
        self.maximum_frames
    }
}

/// One supported stream-configuration range for one direction.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioFormatRange {
    channels: u16,
    minimum_sample_rate: u32,
    maximum_sample_rate: u32,
    sample_format: DeviceSampleFormat,
    buffer_capability: AudioBufferCapability,
}

impl AudioFormatRange {
    pub fn new(
        channels: u16,
        minimum_sample_rate: u32,
        maximum_sample_rate: u32,
        sample_format: DeviceSampleFormat,
        buffer_capability: AudioBufferCapability,
    ) -> Result<Self, AudioFormatError> {
        if channels == 0 {
            return Err(AudioFormatError::ZeroChannels);
        }
        if minimum_sample_rate == 0 || maximum_sample_rate == 0 {
            return Err(AudioFormatError::ZeroSampleRate);
        }
        if minimum_sample_rate > maximum_sample_rate {
            return Err(AudioFormatError::InvertedSampleRateRange);
        }
        Ok(Self {
            channels,
            minimum_sample_rate,
            maximum_sample_rate,
            sample_format,
            buffer_capability,
        })
    }

    pub const fn channels(self) -> u16 {
        self.channels
    }

    pub const fn minimum_sample_rate(self) -> u32 {
        self.minimum_sample_rate
    }

    pub const fn maximum_sample_rate(self) -> u32 {
        self.maximum_sample_rate
    }

    pub const fn sample_format(self) -> DeviceSampleFormat {
        self.sample_format
    }

    pub const fn buffer_capability(self) -> AudioBufferCapability {
        self.buffer_capability
    }

    pub const fn contains_sample_rate(self, sample_rate: u32) -> bool {
        self.minimum_sample_rate <= sample_rate && sample_rate <= self.maximum_sample_rate
    }
}

/// Exact format chosen from one advertised capability range.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SelectedAudioFormat {
    channels: u16,
    sample_rate: u32,
    sample_format: DeviceSampleFormat,
    buffer_capability: AudioBufferCapability,
}

impl SelectedAudioFormat {
    pub(crate) const fn new(
        channels: u16,
        sample_rate: u32,
        sample_format: DeviceSampleFormat,
        buffer_capability: AudioBufferCapability,
    ) -> Self {
        Self {
            channels,
            sample_rate,
            sample_format,
            buffer_capability,
        }
    }

    pub const fn channels(self) -> u16 {
        self.channels
    }

    pub const fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    pub const fn sample_format(self) -> DeviceSampleFormat {
        self.sample_format
    }

    pub const fn buffer_capability(self) -> AudioBufferCapability {
        self.buffer_capability
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioBufferCapability, AudioBufferRange, AudioFormatRange, DeviceSampleFormat};
    use crate::AudioFormatError;

    #[test]
    fn buffer_ranges_are_checked() {
        assert_eq!(
            AudioBufferRange::new(0, 1),
            Err(AudioFormatError::ZeroBufferFrames)
        );
        assert_eq!(
            AudioBufferRange::new(2, 1),
            Err(AudioFormatError::InvertedBufferRange)
        );
        let range = AudioBufferRange::new(64, 512).expect("valid range");
        assert_eq!(range.minimum_frames(), 64);
        assert_eq!(range.maximum_frames(), 512);
    }

    #[test]
    fn format_ranges_are_checked_and_report_capabilities() {
        assert_eq!(
            AudioFormatRange::new(
                0,
                44_100,
                48_000,
                DeviceSampleFormat::I16,
                AudioBufferCapability::Unknown
            ),
            Err(AudioFormatError::ZeroChannels)
        );
        assert_eq!(
            AudioFormatRange::new(
                1,
                0,
                48_000,
                DeviceSampleFormat::I16,
                AudioBufferCapability::Unknown
            ),
            Err(AudioFormatError::ZeroSampleRate)
        );
        assert_eq!(
            AudioFormatRange::new(
                1,
                48_000,
                44_100,
                DeviceSampleFormat::I16,
                AudioBufferCapability::Unknown
            ),
            Err(AudioFormatError::InvertedSampleRateRange)
        );

        let range = AudioFormatRange::new(
            2,
            44_100,
            96_000,
            DeviceSampleFormat::F32,
            AudioBufferCapability::Range(
                AudioBufferRange::new(64, 1024).expect("valid buffer range"),
            ),
        )
        .expect("valid format");
        assert_eq!(range.channels(), 2);
        assert!(range.contains_sample_rate(48_000));
        assert!(!range.contains_sample_rate(192_000));
        assert!(range.sample_format().is_selectable());
        assert!(!DeviceSampleFormat::F64.is_selectable());
    }
}
