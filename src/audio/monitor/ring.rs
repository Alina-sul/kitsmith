//! Lock-free single-producer/single-consumer ring buffer for f32 samples.
//!
//! Built for the realtime audio callbacks: no locks, no allocation after
//! construction. Samples are stored as `AtomicU32` bit patterns so the whole
//! structure stays in safe Rust; head/tail are monotonically increasing
//! counters (masked only when indexing), so full/empty never ambiguates.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared {
    buf: Box<[AtomicU32]>,
    /// Total samples ever written. Only the producer advances it.
    head: AtomicUsize,
    /// Total samples ever read. Only the consumer advances it.
    tail: AtomicUsize,
}

/// Write half — owned by the audio *input* callback.
pub struct Producer {
    shared: Arc<Shared>,
}

/// Read half — owned by the audio *output* callback.
pub struct Consumer {
    shared: Arc<Shared>,
}

/// Creates a ring holding `capacity` samples, rounded up to a power of two
/// (at least 2, so the index mask is valid).
///
/// `capacity`: requested size in samples.
/// Returns the producer/consumer pair sharing the buffer.
pub fn ring(capacity: usize) -> (Producer, Consumer) {
    let capacity = capacity.max(2).next_power_of_two();
    let buf = (0..capacity).map(|_| AtomicU32::new(0)).collect();
    let shared = Arc::new(Shared {
        buf,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer { shared },
    )
}

impl Producer {
    /// Pushes as many samples from `src` as fit; returns how many were pushed.
    /// Never overwrites unread data (a full ring drops the incoming excess —
    /// the caller counts that as an overrun).
    pub fn push_slice(&self, src: &[f32]) -> usize {
        let shared = &self.shared;
        let head = shared.head.load(Ordering::Relaxed);
        let tail = shared.tail.load(Ordering::Acquire);
        let free = shared.buf.len() - (head - tail);
        let n = src.len().min(free);
        let mask = shared.buf.len() - 1;
        for (i, sample) in src[..n].iter().enumerate() {
            shared.buf[(head + i) & mask].store(sample.to_bits(), Ordering::Relaxed);
        }
        shared.head.store(head + n, Ordering::Release);
        n
    }
}

impl Consumer {
    /// Pops up to `dst.len()` samples; returns how many were written to `dst`.
    pub fn pop_slice(&self, dst: &mut [f32]) -> usize {
        let shared = &self.shared;
        let tail = shared.tail.load(Ordering::Relaxed);
        let head = shared.head.load(Ordering::Acquire);
        let n = dst.len().min(head - tail);
        let mask = shared.buf.len() - 1;
        for (i, slot) in dst[..n].iter_mut().enumerate() {
            *slot = f32::from_bits(shared.buf[(tail + i) & mask].load(Ordering::Relaxed));
        }
        shared.tail.store(tail + n, Ordering::Release);
        n
    }

    /// Samples currently buffered and readable.
    pub fn available(&self) -> usize {
        self.shared.head.load(Ordering::Acquire) - self.shared.tail.load(Ordering::Relaxed)
    }
}
