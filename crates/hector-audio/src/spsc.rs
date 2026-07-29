use crate::{
    counters::{AudioCounterReader, AudioCounters},
    error::{AudioBufferError, PopError, PushError},
};
use std::{
    alloc::Layout,
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

type Slot = UnsafeCell<MaybeUninit<i16>>;

struct Inner {
    slots: Box<[Slot]>,
    read_index: AtomicUsize,
    write_index: AtomicUsize,
    capacity_samples: usize,
    counters: Arc<AudioCounters>,
}

// SAFETY: `AudioProducer` is the only writer of sample slots and
// `write_index`; `AudioConsumer` is the only reader of sample slots and writer
// of `read_index`. A producer Release-publishes initialized slots before a
// consumer Acquire-reads them. A consumer Release-publishes freed slots before
// producer reuse. Endpoint types are not cloneable, so these roles remain
// exclusive. Counter mutation also has one writer per counter.
unsafe impl Sync for Inner {}

impl Inner {
    fn storage_len(&self) -> usize {
        self.slots.len()
    }

    fn next_index(&self, index: usize) -> usize {
        if index + 1 == self.storage_len() {
            0
        } else {
            index + 1
        }
    }
}

/// Factory for a bounded single-producer, single-consumer audio sample ring.
pub struct AudioSpscBuffer {
    _private: (),
}

impl AudioSpscBuffer {
    /// Allocates a queue whose usable capacity is exactly `capacity_samples`.
    #[allow(clippy::new_ret_no_self)] // Construction intentionally returns role-separated endpoints.
    pub fn new(
        capacity_samples: usize,
    ) -> Result<(AudioProducer, AudioConsumer, AudioCounterReader), AudioBufferError> {
        build_with_reservation(capacity_samples, |slots, storage_len| {
            slots
                .try_reserve_exact(storage_len)
                .map_err(|_| AudioBufferError::AllocationFailed)
        })
    }
}

fn build_with_reservation<F>(
    capacity_samples: usize,
    reserve: F,
) -> Result<(AudioProducer, AudioConsumer, AudioCounterReader), AudioBufferError>
where
    F: FnOnce(&mut Vec<Slot>, usize) -> Result<(), AudioBufferError>,
{
    if capacity_samples == 0 {
        return Err(AudioBufferError::ZeroCapacity);
    }

    let storage_len = capacity_samples
        .checked_add(1)
        .ok_or(AudioBufferError::CapacityOverflow)?;
    Layout::array::<Slot>(storage_len).map_err(|_| AudioBufferError::CapacityOverflow)?;

    let mut slots = Vec::new();
    reserve(&mut slots, storage_len)?;
    slots.resize_with(storage_len, || UnsafeCell::new(MaybeUninit::uninit()));

    let counters = Arc::new(AudioCounters::new());
    let inner = Arc::new(Inner {
        slots: slots.into_boxed_slice(),
        read_index: AtomicUsize::new(0),
        write_index: AtomicUsize::new(0),
        capacity_samples,
        counters: Arc::clone(&counters),
    });

    Ok((
        AudioProducer {
            inner: Arc::clone(&inner),
        },
        AudioConsumer { inner },
        AudioCounterReader::new(counters),
    ))
}

/// Exclusive producer endpoint for a bounded audio sample ring.
pub struct AudioProducer {
    inner: Arc<Inner>,
}

impl AudioProducer {
    /// Returns the exact usable capacity in individual samples.
    pub fn capacity_samples(&self) -> usize {
        self.inner.capacity_samples
    }

    /// Attempts to publish one sample without blocking.
    pub fn try_push(&mut self, sample: i16) -> Result<(), PushError> {
        if self.push_one(sample) {
            Ok(())
        } else {
            self.inner.counters.add_rejected(1);
            Err(PushError::Full(sample))
        }
    }

    /// Publishes the longest prefix that currently fits and returns its length.
    pub fn push_slice(&mut self, samples: &[i16]) -> usize {
        let mut written = 0;
        while written < samples.len() && self.push_one(samples[written]) {
            written += 1;
        }
        self.inner.counters.add_rejected(samples.len() - written);
        written
    }

    fn push_one(&mut self, sample: i16) -> bool {
        let write_index = self.inner.write_index.load(Ordering::Relaxed);
        let next_write = self.inner.next_index(write_index);
        if next_write == self.inner.read_index.load(Ordering::Acquire) {
            return false;
        }

        // SAFETY: `write_index` is always within the allocated ring. This
        // producer exclusively writes the slot, and the Acquire read above
        // proves the consumer released it before reuse.
        unsafe {
            (*self.inner.slots[write_index].get()).write(sample);
        }
        self.inner.write_index.store(next_write, Ordering::Release);
        true
    }
}

/// Exclusive consumer endpoint for a bounded audio sample ring.
pub struct AudioConsumer {
    inner: Arc<Inner>,
}

impl AudioConsumer {
    /// Returns the exact usable capacity in individual samples.
    pub fn capacity_samples(&self) -> usize {
        self.inner.capacity_samples
    }

    /// Attempts to consume one sample without blocking.
    pub fn try_pop(&mut self) -> Result<i16, PopError> {
        match self.pop_one() {
            Some(sample) => Ok(sample),
            None => {
                self.inner.counters.add_missing(1);
                Err(PopError::Empty)
            }
        }
    }

    /// Copies available real samples and returns the number copied.
    ///
    /// Any unavailable suffix remains unchanged and is not counted as an
    /// underrun.
    pub fn pop_slice(&mut self, output: &mut [i16]) -> usize {
        let mut read = 0;
        while read < output.len() {
            match self.pop_one() {
                Some(sample) => {
                    output[read] = sample;
                    read += 1;
                }
                None => break,
            }
        }
        read
    }

    /// Copies available samples, fills the missing suffix with silence, and
    /// returns the number of real samples copied.
    pub fn pop_slice_or_silence(&mut self, output: &mut [i16]) -> usize {
        let read = self.pop_slice(output);
        output[read..].fill(0);
        self.inner.counters.add_missing(output.len() - read);
        read
    }

    fn pop_one(&mut self) -> Option<i16> {
        let read_index = self.inner.read_index.load(Ordering::Relaxed);
        if read_index == self.inner.write_index.load(Ordering::Acquire) {
            return None;
        }

        // SAFETY: `read_index` is within the ring, and observing a distinct
        // write index with Acquire proves the producer initialized this slot
        // before its Release publication. This sole consumer reads it once
        // before releasing the slot for producer reuse.
        let sample = unsafe { (*self.inner.slots[read_index].get()).assume_init_read() };
        self.inner
            .read_index
            .store(self.inner.next_index(read_index), Ordering::Release);
        Some(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioSpscBuffer, Slot, build_with_reservation};
    use crate::{AudioBufferError, PopError, PushError};
    use std::{mem::size_of, sync::mpsc, thread};

    #[test]
    fn construction_rejects_zero_overflow_and_reservation_failure() {
        assert!(matches!(
            AudioSpscBuffer::new(0),
            Err(AudioBufferError::ZeroCapacity)
        ));
        assert!(matches!(
            AudioSpscBuffer::new(usize::MAX),
            Err(AudioBufferError::CapacityOverflow)
        ));
        let layout_overflow = (isize::MAX as usize / size_of::<Slot>()) + 1;
        assert!(matches!(
            AudioSpscBuffer::new(layout_overflow),
            Err(AudioBufferError::CapacityOverflow)
        ));

        let result = build_with_reservation(8, |_: &mut Vec<Slot>, _| {
            Err(AudioBufferError::AllocationFailed)
        });
        assert!(matches!(result, Err(AudioBufferError::AllocationFailed)));
    }

    #[test]
    fn requested_capacity_is_exact_for_arbitrary_positive_sizes() {
        for capacity in [1, 2, 3, 7, 16] {
            let (producer, consumer, counters) =
                AudioSpscBuffer::new(capacity).expect("small allocation succeeds");
            assert_eq!(producer.capacity_samples(), capacity);
            assert_eq!(consumer.capacity_samples(), capacity);
            assert_eq!(counters.rejected_samples(), 0);
            assert_eq!(counters.missing_samples(), 0);
        }
    }

    #[test]
    fn single_sample_operations_preserve_fifo_and_pressure() {
        let (mut producer, mut consumer, counters) =
            AudioSpscBuffer::new(2).expect("allocation succeeds");

        assert_eq!(consumer.try_pop(), Err(PopError::Empty));
        assert_eq!(producer.try_push(10), Ok(()));
        assert_eq!(producer.try_push(-20), Ok(()));
        assert_eq!(producer.try_push(30), Err(PushError::Full(30)));
        assert_eq!(consumer.try_pop(), Ok(10));
        assert_eq!(consumer.try_pop(), Ok(-20));
        assert_eq!(consumer.try_pop(), Err(PopError::Empty));
        assert_eq!(counters.rejected_samples(), 1);
        assert_eq!(counters.missing_samples(), 2);
    }

    #[test]
    fn bulk_operations_count_exact_suffixes_and_preserve_output() {
        let (mut producer, mut consumer, counters) =
            AudioSpscBuffer::new(3).expect("allocation succeeds");

        assert_eq!(producer.push_slice(&[1, 2, 3, 4, 5]), 3);
        assert_eq!(counters.rejected_samples(), 2);

        let mut first = [99; 5];
        assert_eq!(consumer.pop_slice(&mut first), 3);
        assert_eq!(first, [1, 2, 3, 99, 99]);
        assert_eq!(counters.missing_samples(), 0);

        assert_eq!(producer.push_slice(&[7, 8]), 2);
        let mut second = [99; 4];
        assert_eq!(consumer.pop_slice_or_silence(&mut second), 2);
        assert_eq!(second, [7, 8, 0, 0]);
        assert_eq!(counters.missing_samples(), 2);

        assert_eq!(producer.push_slice(&[]), 0);
        assert_eq!(consumer.pop_slice(&mut []), 0);
        assert_eq!(consumer.pop_slice_or_silence(&mut []), 0);
        assert_eq!(counters.snapshot().rejected_samples(), 2);
        assert_eq!(counters.snapshot().missing_samples(), 2);
    }

    #[test]
    fn repeated_wraparound_preserves_every_accepted_sample_once() {
        let (mut producer, mut consumer, _) = AudioSpscBuffer::new(3).expect("allocation succeeds");

        for cycle in 0..1_000_i16 {
            let base = cycle * 3;
            assert_eq!(producer.push_slice(&[base, base + 1, base + 2]), 3);
            let mut output = [0; 3];
            assert_eq!(consumer.pop_slice(&mut output), 3);
            assert_eq!(output, [base, base + 1, base + 2]);
        }
    }

    #[test]
    fn producer_and_consumer_preserve_fifo_under_thread_skew() {
        const COUNT: i16 = 20_000;
        let (mut producer, mut consumer, _counters) =
            AudioSpscBuffer::new(31).expect("allocation succeeds");
        let (start_sender, start_receiver) = mpsc::channel();

        let producer_thread = thread::spawn(move || {
            start_receiver.recv().expect("consumer starts");
            for sample in 0..COUNT {
                let mut pending = sample;
                loop {
                    match producer.try_push(pending) {
                        Ok(()) => break,
                        Err(PushError::Full(rejected)) => {
                            pending = rejected;
                            thread::yield_now();
                        }
                    }
                }
            }
        });

        let consumer_thread = thread::spawn(move || {
            start_sender.send(()).expect("producer starts");
            let mut received = Vec::with_capacity(COUNT as usize);
            while received.len() < COUNT as usize {
                match consumer.try_pop() {
                    Ok(sample) => received.push(sample),
                    Err(PopError::Empty) => thread::yield_now(),
                }
            }
            received
        });

        producer_thread.join().expect("producer completes");
        let received = consumer_thread.join().expect("consumer completes");
        assert_eq!(received, (0..COUNT).collect::<Vec<_>>());
    }

    #[test]
    fn endpoint_and_reader_drop_orders_keep_storage_valid() {
        let (mut producer, consumer, reader) =
            AudioSpscBuffer::new(1).expect("allocation succeeds");
        drop(consumer);
        assert_eq!(producer.try_push(1), Ok(()));
        drop(reader);
        drop(producer);

        let (producer, mut consumer, reader) =
            AudioSpscBuffer::new(1).expect("allocation succeeds");
        drop(producer);
        assert_eq!(consumer.try_pop(), Err(PopError::Empty));
        drop(consumer);
        assert_eq!(reader.missing_samples(), 1);
    }
}
