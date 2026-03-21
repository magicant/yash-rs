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
use std::rc::Rc;
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
    /// The waker is wrapped in `Rc<Cell<Option<Waker>>>` to allow it to be
    /// shared among multiple wake conditions. When the waker is
    /// [woken up](Waker::wake), it will be taken from the `Cell` and set to
    /// `None`, indicating that the process has been woken up and the item can
    /// be removed from the queue.
    ///
    /// This method may (or may not) remove wakers that have already been set to
    /// `None` by other wake conditions, reclaiming the memory used by those
    /// items.
    pub fn push(&mut self, wake_time: Instant, waker: Rc<Cell<Option<Waker>>>) {
        let old_capacity = self.0.capacity();

        self.0.push(ScheduledWaker { wake_time, waker });

        // If the capacity has increased, it means the inner heap performed an
        // O(n) reallocation, which is a good opportunity to clean up empty
        // wakers (which also costs O(n)). This way we can amortize the cleanup
        // cost over multiple insertions, and avoid doing cleanup on every
        // insertion.
        if self.0.capacity() > old_capacity {
            self.0.retain(|item| {
                // Since `Waker` is not `Copy`, we need to take it from the
                // `Cell` to check if it's `None`, and put it back afterward.
                // item.waker.get().is_some()
                let waker = item.waker.take();
                let is_some = waker.is_some();
                item.waker.set(waker);
                is_some
            });
        }
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
            if let Some(waker) = item.waker.take() {
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
    /// The waker is shared in `Rc` to allow other wake conditions to share the
    /// same waker, and wrapped in `Cell` of `Option` to allow it to be taken
    /// when waking up the process. When the waker is `None`, it means the
    /// process has already been woken up (possibly by other conditions) and the
    /// item can be removed from the queue.
    #[debug(ignore)]
    #[eq(ignore)]
    #[partial_eq(ignore)]
    pub waker: Rc<Cell<Option<Waker>>>,
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

        queue.push(now + Duration::from_secs(5), dummy_waker());
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(5)));

        queue.push(now + Duration::from_secs(3), dummy_waker());
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));

        queue.push(now + Duration::from_secs(10), dummy_waker());
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
        queue.push(now + Duration::from_secs(3), waker_1);
        queue.push(now + Duration::from_secs(5), waker_2);
        queue.push(now + Duration::from_secs(6), waker_3);

        // The first two wakers should be triggered, but not the third one
        queue.wake(now + Duration::from_secs(5));
        assert!(wake_flag_1.is_woken());
        assert!(wake_flag_2.is_woken());
        assert!(!wake_flag_3.is_woken());
    }
}
