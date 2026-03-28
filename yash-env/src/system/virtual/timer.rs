// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Items for managing timers in the virtual system

use derive_more::{Debug, Eq, PartialEq};
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;
use std::rc::Weak;
use std::task::Waker;
use std::time::Instant;

/// Priority queue of scheduled wakers to wake up processes at specific times
///
/// This struct represents a priority queue of scheduled wakers, where each
/// waker is associated with a specific time at which a process should be woken
/// up.
#[derive(Clone, Debug, Default)]
pub struct ScheduledWakerQueue(BinaryHeap<ScheduledWaker>);

impl ScheduledWakerQueue {
    /// Creates a new empty `ScheduledWakerQueue`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self(BinaryHeap::new())
    }

    /// Pushes a new scheduled waker into the queue.
    ///
    /// This method adds a new scheduled waker to the priority queue so that the
    /// associated process can be woken up at the specified time.
    ///
    /// The waker is wrapped in `Weak<Cell<Option<Waker>>>` to allow it to be
    /// shared among multiple wake conditions. When the waker is
    /// [woken up](Waker::wake), it will be taken from the `Cell` and set to
    /// `None`, indicating that the process has been woken up and the item can
    /// be removed from the queue. The strong reference count of the `Weak`
    /// waker can also be dropped to `0` when the waker is no longer needed,
    /// which also indicates that the process has been woken up or canceled and
    /// the item can be removed from the queue.
    ///
    /// This method may (or may not) remove such invalidated wakers to reclaim
    /// the memory.
    pub fn push(&mut self, wake_time: Instant, waker: Weak<Cell<Option<Waker>>>) {
        // Helper closure to check if a scheduled waker is still alive
        // (i.e., has a strong reference and the contained waker is not `None`)
        let is_alive = |scheduled_waker: &ScheduledWaker| {
            scheduled_waker.waker.upgrade().is_some_and(|cell| {
                // Since `Waker` is not `Copy`, we need to take it from the
                // `Cell` to check if it's `None`, and put it back afterward.
                let waker = cell.take();
                let is_alive = waker.is_some();
                cell.set(waker);
                is_alive
            })
        };

        // Before pushing the new item, remove any expired items from the front
        // of the queue.
        while let Some(item) = self.0.peek_mut() {
            if is_alive(&item) {
                break;
            }
            PeekMut::pop(item);
        }

        if self.0.capacity() == self.0.len() {
            // If the queue is full, the inner heap will need to perform an O(n)
            // reallocation to accommodate the new item. This is a good
            // opportunity to clean up empty wakers (which also costs O(n)).
            // This way we can amortize the cleanup cost over multiple
            // insertions, and avoid doing cleanup on every insertion.
            self.0.retain(is_alive);

            // If we removed substantial number of items, we can also shrink
            // the capacity to save memory and create more opportunities for
            // cleanup in the future.
            self.0.shrink_to(std::cmp::max(8, self.0.len() * 2));

            // Make sure the next cleanup does not occur within next `len`
            // insertions for amortization.
            self.0.reserve(self.0.len());
        }

        self.0.push(ScheduledWaker { wake_time, waker });
    }

    /// Wakes up processes whose scheduled wake time has been reached.
    ///
    /// This method checks the priority queue for any scheduled wakers whose
    /// wake time is less than or equal to the current time (`now`). For each
    /// such waker, it takes the waker from the `Cell` and calls `wake()` on it
    /// to wake up the associated process. After waking up the process, the item
    /// is removed from the queue.
    pub fn wake(&mut self, now: Instant) {
        while let Some(item) = self.0.peek_mut() {
            if item.wake_time > now {
                break;
            }
            let item = PeekMut::pop(item);
            if let Some(waker) = item.waker.upgrade().and_then(|cell| cell.take()) {
                waker.wake();
            }
        }
    }

    /// Returns the next scheduled wake time, if any.
    ///
    /// This method peeks at the priority queue to find the scheduled waker with
    /// the earliest wake time. If the queue is not empty, it returns the wake
    /// time of that waker; otherwise, it returns `None`.
    pub fn next_wake_time(&self) -> Option<Instant> {
        self.0.peek().map(|item| item.wake_time)
    }
}

/// Priority queue item for waking up processes at a specific time
///
/// This struct represents an item in [`ScheduledWakerQueue`] used for managing
/// timers in the virtual system. Each item contains the time at which a process
/// should be woken up and a waker that can be used to wake up the process when
/// the time is reached.
///
/// The implementation of `Ord` for `ScheduledWaker` compares the `wake_time`
/// fields in reverse order, so that the item with the earliest `wake_time` is
/// considered the greatest in [`std::collections::BinaryHeap`].
#[derive(Clone, Debug, Eq, PartialEq)]
struct ScheduledWaker {
    /// Time to wake up
    pub wake_time: Instant,

    /// Waker to wake up the virtual process
    ///
    /// The waker is shared as a weak reference to allow other wake conditions
    /// to activate the same waker, and wrapped in `Cell` of `Option` to allow
    /// it to be taken when waking up the process. When the weak reference has
    /// no strong references or the waker is `None`, it means the process has
    /// already been woken up (possibly by other conditions) or canceled, and
    /// the item can be removed from the queue.
    #[eq(ignore)]
    #[partial_eq(ignore)]
    pub waker: Weak<Cell<Option<Waker>>>,
}

/// Compares `wake_time` in reverse order to make the earliest wake time the greatest
impl PartialOrd for ScheduledWaker {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Reverse the order to make the earliest wake_time the greatest
        Some(self.cmp(other))
    }
}

/// Compares `wake_time` in reverse order to make the earliest wake time the greatest
impl Ord for ScheduledWaker {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse the order to make the earliest wake_time the greatest
        other.wake_time.cmp(&self.wake_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helper::WakeFlag;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Duration;

    fn dummy_waker() -> Rc<Cell<Option<Waker>>> {
        Rc::new(Cell::new(Some(Waker::noop().clone())))
    }

    #[test]
    fn next_wake_time_returns_none_if_empty() {
        let queue = ScheduledWakerQueue::new();
        assert_eq!(queue.next_wake_time(), None);
    }

    #[test]
    fn next_wake_time_returns_earliest_pending_waker_time() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();
        let waker = dummy_waker();

        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker));
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(5)));

        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker));
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));

        queue.push(now + Duration::from_secs(10), Rc::downgrade(&waker));
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));
    }

    #[test]
    fn wake_triggers_all_wakers_up_to_now() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let wake_flag_1 = Arc::new(WakeFlag::new());
        let wake_flag_2 = Arc::new(WakeFlag::new());
        let wake_flag_3 = Arc::new(WakeFlag::new());
        let waker_1 = Rc::new(Cell::new(Some(Waker::from(wake_flag_1.clone()))));
        let waker_2 = Rc::new(Cell::new(Some(Waker::from(wake_flag_2.clone()))));
        let waker_3 = Rc::new(Cell::new(Some(Waker::from(wake_flag_3.clone()))));
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(6), Rc::downgrade(&waker_3));

        // The first two wakers should be triggered, but not the third one
        queue.wake(now + Duration::from_secs(5));
        assert!(wake_flag_1.is_woken());
        assert!(wake_flag_2.is_woken());
        assert!(!wake_flag_3.is_woken());
    }

    #[test]
    fn complex_pushes_and_wakes() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let wake_flag_1 = Arc::new(WakeFlag::new());
        let wake_flag_2 = Arc::new(WakeFlag::new());
        let wake_flag_3 = Arc::new(WakeFlag::new());
        let waker_1 = Rc::new(Cell::new(Some(Waker::from(wake_flag_1.clone()))));
        let waker_2 = Rc::new(Cell::new(Some(Waker::from(wake_flag_2.clone()))));
        let waker_3 = Rc::new(Cell::new(Some(Waker::from(wake_flag_3.clone()))));

        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(10), Rc::downgrade(&waker_3));

        // The first two wakers should be triggered, but not the third one
        queue.wake(now + Duration::from_secs(5));
        assert!(wake_flag_1.is_woken());
        assert!(wake_flag_2.is_woken());
        assert!(!wake_flag_3.is_woken());

        // After waking, the next wake time should be the third one
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(10)));

        // The third waker should be triggered now
        queue.wake(now + Duration::from_secs(15));
        assert!(wake_flag_3.is_woken());

        // After waking all, the next wake time should be None
        assert_eq!(queue.next_wake_time(), None);
    }

    #[test]
    fn push_trims_expired_entries() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let wake_flag_1 = Arc::new(WakeFlag::new());
        let wake_flag_2 = Arc::new(WakeFlag::new());
        let wake_flag_3 = Arc::new(WakeFlag::new());
        let wake_flag_4 = Arc::new(WakeFlag::new());
        let waker_1 = Rc::new(Cell::new(Some(Waker::from(wake_flag_1))));
        let waker_2 = Rc::new(Cell::new(Some(Waker::from(wake_flag_2))));
        let waker_3 = Rc::new(Cell::new(Some(Waker::from(wake_flag_3.clone()))));
        let waker_4 = Rc::new(Cell::new(Some(Waker::from(wake_flag_4.clone()))));

        queue.0.reserve(4);
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(7), Rc::downgrade(&waker_3));

        // Manually wake the first two wakers to simulate them being woken by other conditions
        waker_1.take().unwrap().wake();
        waker_2.take().unwrap().wake();

        // Now push a new waker, which should trigger the cleanup of the expired entries
        queue.push(now + Duration::from_secs(1), Rc::downgrade(&waker_4));
        assert_eq!(queue.0.len(), 2);

        // The remaining wakers should be the third and fourth ones
        queue.wake(now + Duration::from_secs(10));
        assert!(wake_flag_3.is_woken());
        assert!(wake_flag_4.is_woken());
    }

    #[test]
    fn push_occasionally_cleans_up_expired_entries() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let waker_1 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_2 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_3 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_4 = Rc::new(Cell::new(Some(Waker::noop().clone())));

        queue.0.reserve(10);
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_1));
        while queue.0.len() + 1 < queue.0.capacity() {
            queue.push(now + Duration::new(3, queue.0.len() as u32), Weak::new());
        }
        queue.push(now + Duration::from_secs(4), Rc::downgrade(&waker_2));
        assert_eq!(queue.0.len(), queue.0.capacity());

        // The next push should trigger cleanup of expired entries
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_3));
        assert_eq!(queue.0.len(), 3);

        // Manually wake the last waker to simulate it being woken by other conditions
        waker_3.take().unwrap().wake();

        // Another push does not trigger cleanup since the capacity is not yet reached
        queue.push(now + Duration::from_secs(6), Rc::downgrade(&waker_4));
        assert_eq!(queue.0.len(), 4);
    }
}
