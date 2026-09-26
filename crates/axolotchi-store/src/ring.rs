//! Raw sightings never touch SQLite — only presence *changes* do, per
//! `CLAUDE.md`'s SD-card-protecting write rule. This fixed-capacity ring
//! buffer is where they live instead: enough recent history in RAM for
//! debugging or a "recent activity" view, with the oldest entries dropped
//! once it's full.

use std::collections::VecDeque;

use axolotchi_core::{DeviceId, SightingSource, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSighting {
    pub device_id: DeviceId,
    pub ip: Option<String>,
    pub source: SightingSource,
    pub at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    buf: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// # Panics
    /// Panics if `capacity` is zero — a buffer that holds nothing isn't a
    /// useful ring buffer.
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "ring buffer capacity must be greater than zero"
        );
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Pushes an item, dropping the oldest one first if already at capacity.
    pub fn push(&mut self, item: T) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(item);
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Iterates oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushes_within_capacity_keep_everything() {
        let mut buf = RingBuffer::new(3);
        buf.push(1);
        buf.push(2);
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn pushing_past_capacity_drops_the_oldest() {
        let mut buf = RingBuffer::new(3);
        buf.push(1);
        buf.push(2);
        buf.push(3);
        buf.push(4);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn capacity_and_emptiness_report_correctly() {
        let mut buf: RingBuffer<i32> = RingBuffer::new(2);
        assert_eq!(buf.capacity(), 2);
        assert!(buf.is_empty());
        buf.push(1);
        assert!(!buf.is_empty());
    }

    #[test]
    #[should_panic(expected = "capacity must be greater than zero")]
    fn zero_capacity_panics() {
        let _: RingBuffer<i32> = RingBuffer::new(0);
    }

    #[test]
    fn holds_raw_sightings() {
        let mut buf = RingBuffer::new(2);
        buf.push(RawSighting {
            device_id: "dev-1".into(),
            ip: Some("10.0.0.5".into()),
            source: SightingSource::Active,
            at: 1000,
        });
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.iter().next().unwrap().device_id, "dev-1");
    }
}
