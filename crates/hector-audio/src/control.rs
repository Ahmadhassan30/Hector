use hector_core::{AudioEpoch, GenerationEpoch};
use std::sync::atomic::{AtomicU64, Ordering};

/// Latest observed assistant generation for a later urgent-control owner.
///
/// A single logical publisher and observer are a runtime usage invariant, not
/// a type-system guarantee. Concurrent access is memory-safe, but later stores
/// overwrite earlier stores and no publication history is retained.
#[derive(Debug)]
pub struct UrgentGenerationSlot {
    value: AtomicU64,
}

#[allow(clippy::new_without_default)] // Explicit initialization is part of the slot contract.
impl UrgentGenerationSlot {
    /// Constructs an empty observation slot.
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// Publishes the latest observed generation.
    pub fn publish(&self, generation: GenerationEpoch) {
        self.value.store(generation.get(), Ordering::Release);
    }

    /// Loads the latest observed generation, if one has been published.
    pub fn load(&self) -> Option<GenerationEpoch> {
        GenerationEpoch::from_raw(self.value.load(Ordering::Acquire))
    }
}

/// Latest observed audio discontinuity identity.
///
/// A single logical publisher and observer are a runtime usage invariant, not
/// a type-system guarantee. Concurrent access is memory-safe, but later stores
/// overwrite earlier stores and no publication history is retained. This slot
/// has no assistant-output freshness meaning.
#[derive(Debug)]
pub struct AudioEpochSlot {
    value: AtomicU64,
}

#[allow(clippy::new_without_default)] // Explicit initialization is part of the slot contract.
impl AudioEpochSlot {
    /// Constructs an empty observation slot.
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// Publishes the latest observed audio discontinuity.
    pub fn publish(&self, epoch: AudioEpoch) {
        self.value.store(epoch.get(), Ordering::Release);
    }

    /// Loads the latest observed audio discontinuity, if one has been published.
    pub fn load(&self) -> Option<AudioEpoch> {
        AudioEpoch::from_raw(self.value.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioEpochSlot, UrgentGenerationSlot};
    use hector_core::{AudioEpoch, GenerationEpoch};
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    fn generation(raw: u64) -> GenerationEpoch {
        GenerationEpoch::from_raw(raw).expect("test generation is nonzero")
    }

    fn audio_epoch(raw: u64) -> AudioEpoch {
        AudioEpoch::from_raw(raw).expect("test audio epoch is nonzero")
    }

    #[test]
    fn generation_slot_is_empty_then_preserves_latest_epoch() {
        let slot = UrgentGenerationSlot::new();
        assert_eq!(slot.load(), None);

        slot.publish(generation(1));
        assert_eq!(slot.load(), Some(generation(1)));
        slot.publish(generation(1));
        assert_eq!(slot.load(), Some(generation(1)));
        slot.publish(generation(u64::MAX));
        assert_eq!(slot.load(), Some(generation(u64::MAX)));
    }

    #[test]
    fn audio_slot_is_independent_and_has_no_freshness_effect() {
        let generation_slot = UrgentGenerationSlot::new();
        let audio_slot = AudioEpochSlot::new();

        generation_slot.publish(generation(7));
        audio_slot.publish(audio_epoch(11));
        audio_slot.publish(audio_epoch(12));

        assert_eq!(generation_slot.load(), Some(generation(7)));
        assert_eq!(audio_slot.load(), Some(audio_epoch(12)));
    }

    #[test]
    fn concurrent_publication_and_observation_are_memory_safe() {
        let slot = Arc::new(UrgentGenerationSlot::new());
        let barrier = Arc::new(Barrier::new(2));
        let published = Arc::clone(&slot);
        let published_barrier = Arc::clone(&barrier);
        let publisher = thread::spawn(move || {
            published_barrier.wait();
            for raw in 1..=10_000 {
                published.publish(generation(raw));
            }
        });

        barrier.wait();
        while slot.load() != Some(generation(10_000)) {
            thread::yield_now();
        }
        publisher.join().expect("publisher completes");
        assert_eq!(slot.load(), Some(generation(10_000)));
    }
}
