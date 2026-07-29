use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// A point-in-time view of audio queue pressure measured in samples.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AudioCounterSnapshot {
    rejected_samples: u64,
    missing_samples: u64,
}

impl AudioCounterSnapshot {
    /// Returns the number of samples rejected because the queue was full.
    pub const fn rejected_samples(self) -> u64 {
        self.rejected_samples
    }

    /// Returns the number of requested samples replaced by no data.
    pub const fn missing_samples(self) -> u64 {
        self.missing_samples
    }
}

#[derive(Debug)]
pub(crate) struct AudioCounters {
    rejected_samples: AtomicU64,
    missing_samples: AtomicU64,
}

impl AudioCounters {
    pub(crate) const fn new() -> Self {
        Self {
            rejected_samples: AtomicU64::new(0),
            missing_samples: AtomicU64::new(0),
        }
    }

    pub(crate) fn add_rejected(&self, count: usize) {
        saturating_add(&self.rejected_samples, count);
    }

    pub(crate) fn add_missing(&self, count: usize) {
        saturating_add(&self.missing_samples, count);
    }

    fn snapshot(&self) -> AudioCounterSnapshot {
        AudioCounterSnapshot {
            rejected_samples: self.rejected_samples.load(Ordering::Relaxed),
            missing_samples: self.missing_samples.load(Ordering::Relaxed),
        }
    }
}

fn saturating_add(counter: &AtomicU64, count: usize) {
    if count == 0 {
        return;
    }

    let increment = u64::try_from(count).unwrap_or(u64::MAX);
    let current = counter.load(Ordering::Relaxed);
    counter.store(current.saturating_add(increment), Ordering::Relaxed);
}

/// Read-only access to audio queue pressure counters.
#[derive(Clone, Debug)]
pub struct AudioCounterReader {
    counters: Arc<AudioCounters>,
}

impl AudioCounterReader {
    pub(crate) fn new(counters: Arc<AudioCounters>) -> Self {
        Self { counters }
    }

    /// Returns the saturated count of samples rejected by a full queue.
    pub fn rejected_samples(&self) -> u64 {
        self.counters.snapshot().rejected_samples()
    }

    /// Returns the saturated count of requested samples that were unavailable.
    pub fn missing_samples(&self) -> u64 {
        self.counters.snapshot().missing_samples()
    }

    /// Returns both pressure counters from one bounded observation.
    pub fn snapshot(&self) -> AudioCounterSnapshot {
        self.counters.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioCounterReader, AudioCounters, saturating_add};
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    #[test]
    fn counters_initialize_at_zero_and_snapshot_exact_samples() {
        let counters = Arc::new(AudioCounters::new());
        let reader = AudioCounterReader::new(Arc::clone(&counters));

        assert_eq!(reader.rejected_samples(), 0);
        assert_eq!(reader.missing_samples(), 0);

        counters.add_rejected(3);
        counters.add_missing(5);
        let snapshot = reader.snapshot();
        assert_eq!(snapshot.rejected_samples(), 3);
        assert_eq!(snapshot.missing_samples(), 5);
        assert_eq!(reader.clone().snapshot(), snapshot);
    }

    #[test]
    fn increments_saturate_without_wrapping() {
        let counter = AtomicU64::new(u64::MAX - 1);
        saturating_add(&counter, 10);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
        saturating_add(&counter, 1);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
        saturating_add(&counter, 0);
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
}
