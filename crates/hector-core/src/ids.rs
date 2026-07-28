use core::{
    fmt,
    num::{NonZeroU64, NonZeroU128},
};

/// Persistent identity of a conversation session.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SessionId(NonZeroU128);

impl SessionId {
    /// Constructs a session identifier from a nonzero raw value.
    pub const fn from_raw(raw: u128) -> Option<Self> {
        match NonZeroU128::new(raw) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value of this session identifier.
    pub const fn get(self) -> u128 {
        self.0.get()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Session-local identity of a conversation turn.
///
/// Session locality is a semantic invariant rather than one enforced by this
/// type. Ordering is meaningful only between turns belonging to the same
/// session.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct TurnId(NonZeroU64);

impl TurnId {
    /// Constructs a turn identifier from a nonzero raw value.
    pub const fn from_raw(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value of this turn identifier.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Returns the next turn identifier, or `None` at the numeric limit.
    pub const fn checked_next(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(raw) => Self::from_raw(raw),
            None => None,
        }
    }
}

impl fmt::Display for TurnId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Process-lifetime assistant generation identity.
///
/// This is the sole freshness fence for assistant output.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct GenerationEpoch(NonZeroU64);

impl GenerationEpoch {
    /// Constructs a generation epoch from a nonzero raw value.
    pub const fn from_raw(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value of this generation epoch.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Returns the next generation epoch, or `None` at the numeric limit.
    pub const fn checked_next(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(raw) => Self::from_raw(raw),
            None => None,
        }
    }
}

impl fmt::Display for GenerationEpoch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Identity used only to correlate and trace a request.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RequestId(NonZeroU128);

impl RequestId {
    /// Constructs a request identifier from a nonzero raw value.
    pub const fn from_raw(raw: u128) -> Option<Self> {
        match NonZeroU128::new(raw) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value of this request identifier.
    pub const fn get(self) -> u128 {
        self.0.get()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

/// Identity of an audio capture or playback discontinuity.
///
/// This epoch represents audio continuity only. It is not an assistant-output
/// freshness fence.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct AudioEpoch(NonZeroU64);

impl AudioEpoch {
    /// Constructs an audio epoch from a nonzero raw value.
    pub const fn from_raw(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value of this audio epoch.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Returns the next audio epoch, or `None` at the numeric limit.
    pub const fn checked_next(self) -> Option<Self> {
        match self.get().checked_add(1) {
            Some(raw) => Self::from_raw(raw),
            None => None,
        }
    }
}

impl fmt::Display for AudioEpoch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioEpoch, GenerationEpoch, RequestId, SessionId, TurnId};
    use core::{
        fmt::Debug,
        hash::{Hash, Hasher},
    };
    use std::collections::hash_map::DefaultHasher;

    const SESSION_RAW_FROM_CONST: u128 = match SessionId::from_raw(1) {
        Some(id) => id.get(),
        None => 0,
    };
    const REQUEST_RAW_FROM_CONST: u128 = match RequestId::from_raw(1) {
        Some(id) => id.get(),
        None => 0,
    };
    const TURN_NEXT_FROM_CONST: Option<u64> = match TurnId::from_raw(1) {
        Some(id) => match id.checked_next() {
            Some(next) => Some(next.get()),
            None => None,
        },
        None => None,
    };
    const GENERATION_NEXT_FROM_CONST: Option<u64> = match GenerationEpoch::from_raw(1) {
        Some(id) => match id.checked_next() {
            Some(next) => Some(next.get()),
            None => None,
        },
        None => None,
    };
    const AUDIO_NEXT_FROM_CONST: Option<u64> = match AudioEpoch::from_raw(1) {
        Some(id) => match id.checked_next() {
            Some(next) => Some(next.get()),
            None => None,
        },
        None => None,
    };

    fn hash_of<T: Hash>(value: T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn assert_common_traits<T: Copy + Clone + Debug + Eq + Hash>() {}

    fn assert_order_matches_raw<T>(construct: impl Fn(u64) -> T)
    where
        T: Copy + Debug + Ord,
    {
        const CORPUS: [u64; 6] = [1, 2, 3, 42, u64::MAX - 1, u64::MAX];

        for left_raw in CORPUS {
            for right_raw in CORPUS {
                let left = construct(left_raw);
                let right = construct(right_raw);
                assert_eq!(left.cmp(&right), left_raw.cmp(&right_raw));
            }
        }

        for first_raw in CORPUS {
            for second_raw in CORPUS {
                for third_raw in CORPUS {
                    let first = construct(first_raw);
                    let second = construct(second_raw);
                    let third = construct(third_raw);

                    if first <= second && second <= third {
                        assert!(first <= third);
                    }
                }
            }
        }
    }

    #[test]
    fn zero_is_rejected_for_every_identifier() {
        assert_eq!(SessionId::from_raw(0), None);
        assert_eq!(RequestId::from_raw(0), None);
        assert_eq!(TurnId::from_raw(0), None);
        assert_eq!(GenerationEpoch::from_raw(0), None);
        assert_eq!(AudioEpoch::from_raw(0), None);
    }

    #[test]
    fn constructors_and_accessors_work_in_const_contexts() {
        assert_eq!(SESSION_RAW_FROM_CONST, 1);
        assert_eq!(REQUEST_RAW_FROM_CONST, 1);
        assert_eq!(TURN_NEXT_FROM_CONST, Some(2));
        assert_eq!(GENERATION_NEXT_FROM_CONST, Some(2));
        assert_eq!(AUDIO_NEXT_FROM_CONST, Some(2));
    }

    #[test]
    fn wide_identifiers_round_trip_positive_values() {
        for raw in 1..=1024 {
            assert_eq!(SessionId::from_raw(raw).map(SessionId::get), Some(raw));
            assert_eq!(RequestId::from_raw(raw).map(RequestId::get), Some(raw));
        }

        for raw in [u128::MAX - 1, u128::MAX] {
            assert_eq!(SessionId::from_raw(raw).map(SessionId::get), Some(raw));
            assert_eq!(RequestId::from_raw(raw).map(RequestId::get), Some(raw));
        }
    }

    #[test]
    fn sequential_identifiers_round_trip_positive_values() {
        for raw in 1..=1024 {
            assert_eq!(TurnId::from_raw(raw).map(TurnId::get), Some(raw));
            assert_eq!(
                GenerationEpoch::from_raw(raw).map(GenerationEpoch::get),
                Some(raw)
            );
            assert_eq!(AudioEpoch::from_raw(raw).map(AudioEpoch::get), Some(raw));
        }

        for raw in [u64::MAX - 1, u64::MAX] {
            assert_eq!(TurnId::from_raw(raw).map(TurnId::get), Some(raw));
            assert_eq!(
                GenerationEpoch::from_raw(raw).map(GenerationEpoch::get),
                Some(raw)
            );
            assert_eq!(AudioEpoch::from_raw(raw).map(AudioEpoch::get), Some(raw));
        }
    }

    #[test]
    fn common_traits_and_formatting_are_consistent() {
        assert_common_traits::<SessionId>();
        assert_common_traits::<RequestId>();
        assert_common_traits::<TurnId>();
        assert_common_traits::<GenerationEpoch>();
        assert_common_traits::<AudioEpoch>();

        let session = SessionId::from_raw(7).expect("seven is nonzero");
        let request = RequestId::from_raw(7).expect("seven is nonzero");
        let turn = TurnId::from_raw(7).expect("seven is nonzero");
        let generation = GenerationEpoch::from_raw(7).expect("seven is nonzero");
        let audio = AudioEpoch::from_raw(7).expect("seven is nonzero");

        assert_eq!(session, session);
        assert_eq!(request, request);
        assert_eq!(turn, turn);
        assert_eq!(generation, generation);
        assert_eq!(audio, audio);

        assert_ne!(session, SessionId::from_raw(8).expect("eight is nonzero"));
        assert_ne!(request, RequestId::from_raw(8).expect("eight is nonzero"));
        assert_ne!(turn, TurnId::from_raw(8).expect("eight is nonzero"));
        assert_ne!(
            generation,
            GenerationEpoch::from_raw(8).expect("eight is nonzero")
        );
        assert_ne!(audio, AudioEpoch::from_raw(8).expect("eight is nonzero"));

        assert_eq!(hash_of(session), hash_of(session));
        assert_eq!(hash_of(request), hash_of(request));
        assert_eq!(hash_of(turn), hash_of(turn));
        assert_eq!(hash_of(generation), hash_of(generation));
        assert_eq!(hash_of(audio), hash_of(audio));

        assert_eq!(session.to_string(), "7");
        assert_eq!(request.to_string(), "7");
        assert_eq!(turn.to_string(), "7");
        assert_eq!(generation.to_string(), "7");
        assert_eq!(audio.to_string(), "7");

        assert_eq!(format!("{session:?}"), "SessionId(7)");
        assert_eq!(format!("{request:?}"), "RequestId(7)");
        assert_eq!(format!("{turn:?}"), "TurnId(7)");
        assert_eq!(format!("{generation:?}"), "GenerationEpoch(7)");
        assert_eq!(format!("{audio:?}"), "AudioEpoch(7)");
    }

    #[test]
    fn sequential_ordering_matches_raw_values_and_is_transitive() {
        assert_order_matches_raw(|raw| TurnId::from_raw(raw).expect("corpus is nonzero"));
        assert_order_matches_raw(|raw| GenerationEpoch::from_raw(raw).expect("corpus is nonzero"));
        assert_order_matches_raw(|raw| AudioEpoch::from_raw(raw).expect("corpus is nonzero"));
    }

    #[test]
    fn sequential_advancement_is_checked_and_never_wraps() {
        for raw in 1..=1024 {
            assert_eq!(
                TurnId::from_raw(raw)
                    .and_then(TurnId::checked_next)
                    .map(TurnId::get),
                Some(raw + 1)
            );
            assert_eq!(
                GenerationEpoch::from_raw(raw)
                    .and_then(GenerationEpoch::checked_next)
                    .map(GenerationEpoch::get),
                Some(raw + 1)
            );
            assert_eq!(
                AudioEpoch::from_raw(raw)
                    .and_then(AudioEpoch::checked_next)
                    .map(AudioEpoch::get),
                Some(raw + 1)
            );
        }

        assert_eq!(
            TurnId::from_raw(u64::MAX - 1)
                .and_then(TurnId::checked_next)
                .map(TurnId::get),
            Some(u64::MAX)
        );
        assert_eq!(
            GenerationEpoch::from_raw(u64::MAX - 1)
                .and_then(GenerationEpoch::checked_next)
                .map(GenerationEpoch::get),
            Some(u64::MAX)
        );
        assert_eq!(
            AudioEpoch::from_raw(u64::MAX - 1)
                .and_then(AudioEpoch::checked_next)
                .map(AudioEpoch::get),
            Some(u64::MAX)
        );

        assert_eq!(
            TurnId::from_raw(u64::MAX).and_then(TurnId::checked_next),
            None
        );
        assert_eq!(
            GenerationEpoch::from_raw(u64::MAX).and_then(GenerationEpoch::checked_next),
            None
        );
        assert_eq!(
            AudioEpoch::from_raw(u64::MAX).and_then(AudioEpoch::checked_next),
            None
        );
    }
}
