//! The relay between the audio thread and the measuring thread: a ring of
//! samples the audio thread fills without waiting or locking, and an event
//! the measuring thread sleeps on. One producer, one consumer, by
//! agreement: the audio thread only pushes, the measuring thread only
//! drains and waits.

use std::os::fd::RawFd;
use std::sync::atomic::{AtomicI16, AtomicU64, AtomicUsize, Ordering};

pub struct Relay {
    buf: Vec<AtomicI16>,
    mask: usize,
    /// Samples pushed so far; only the producer writes it.
    head: AtomicUsize,
    /// Samples drained so far; only the consumer writes it.
    tail: AtomicUsize,
    /// Samples that found no room and were dropped.
    dropped: AtomicU64,
    event: RawFd,
}

// The atomics carry the samples; the event is a file descriptor.
unsafe impl Send for Relay {}
unsafe impl Sync for Relay {}

impl Relay {
    /// Room for `capacity` samples, rounded up to a power of two.
    pub fn new(capacity: usize) -> Result<Self, String> {
        let capacity = capacity.max(2).next_power_of_two();
        let event = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if event < 0 {
            return Err(format!("eventfd: {}", std::io::Error::last_os_error()));
        }
        Ok(Self {
            buf: (0..capacity).map(|_| AtomicI16::new(0)).collect(),
            mask: capacity - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            event,
        })
    }

    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Samples waiting to be drained.
    pub fn len(&self) -> usize {
        self.head
            .load(Ordering::Acquire)
            .wrapping_sub(self.tail.load(Ordering::Acquire))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Samples dropped for want of room since the relay was made.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Producer: append what fits, drop the rest, wake the consumer.
    /// Returns how many were taken.
    pub fn push(&self, samples: &[i16]) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let room = self.capacity() - head.wrapping_sub(tail);
        let taken = samples.len().min(room);
        for (i, s) in samples[..taken].iter().enumerate() {
            self.buf[head.wrapping_add(i) & self.mask].store(*s, Ordering::Relaxed);
        }
        self.head.store(head.wrapping_add(taken), Ordering::Release);
        if taken < samples.len() {
            self.dropped
                .fetch_add((samples.len() - taken) as u64, Ordering::Relaxed);
        }
        if taken > 0 {
            let one: u64 = 1;
            unsafe {
                libc::write(self.event, &one as *const u64 as *const libc::c_void, 8);
            }
        }
        taken
    }

    /// Consumer: move everything waiting to the end of `out`. Returns how
    /// many were moved.
    pub fn drain(&self, out: &mut Vec<i16>) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        let n = head.wrapping_sub(tail);
        out.reserve(n);
        for i in 0..n {
            out.push(self.buf[tail.wrapping_add(i) & self.mask].load(Ordering::Relaxed));
        }
        self.tail.store(head, Ordering::Release);
        n
    }

    /// Wake the consumer without pushing, for a stop or a change of stream.
    pub fn wake(&self) {
        let one: u64 = 1;
        unsafe {
            libc::write(self.event, &one as *const u64 as *const libc::c_void, 8);
        }
    }

    /// Consumer: sleep until the producer pushes, or `timeout_ms` passes.
    pub fn wait(&self, timeout_ms: i32) {
        let mut fds = libc::pollfd {
            fd: self.event,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe {
            libc::poll(&mut fds, 1, timeout_ms);
            let mut count: u64 = 0;
            libc::read(self.event, &mut count as *mut u64 as *mut libc::c_void, 8);
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn samples_cross_in_order_and_the_overflow_is_counted() {
        let relay = Relay::new(8).unwrap();
        assert_eq!(relay.capacity(), 8);
        assert_eq!(relay.push(&[1, 2, 3]), 3);
        assert_eq!(relay.len(), 3);
        let mut out = Vec::new();
        assert_eq!(relay.drain(&mut out), 3);
        assert_eq!(out, [1, 2, 3]);
        assert!(relay.is_empty());
        // Ten into eight: two dropped, the first eight kept.
        let ten: Vec<i16> = (10..20).collect();
        assert_eq!(relay.push(&ten), 8);
        assert_eq!(relay.dropped(), 2);
        out.clear();
        relay.drain(&mut out);
        assert_eq!(out, (10..18).collect::<Vec<i16>>());
        // Wrapping around the ring keeps the order.
        relay.push(&[7, 7, 7, 7, 7]);
        out.clear();
        relay.drain(&mut out);
        relay.push(&[1, 2, 3, 4, 5, 6]);
        out.clear();
        relay.drain(&mut out);
        assert_eq!(out, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn the_consumer_wakes_when_the_producer_pushes() {
        let relay = Arc::new(Relay::new(1024).unwrap());
        let producer = relay.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            producer.push(&[42; 100]);
        });
        let started = Instant::now();
        let mut out = Vec::new();
        while relay.drain(&mut out) == 0 && started.elapsed() < Duration::from_secs(2) {
            relay.wait(500);
        }
        handle.join().unwrap();
        assert_eq!(out.len(), 100);
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "woke by the event, not the timeout"
        );
        // Without a push, the wait ends at the timeout.
        let started = Instant::now();
        relay.wait(20);
        assert!(started.elapsed() >= Duration::from_millis(15));
    }
}
