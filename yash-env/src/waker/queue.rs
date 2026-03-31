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

//! Items for managing time-based wakers in the virtual system

use super::WakerEntry;
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Weak;
use std::task::Waker;
use std::time::Instant;

/// Priority queue of scheduled wakers to wake up processes at specific times
///
/// This struct represents a priority queue of scheduled wakers, where each
/// waker is associated with a specific time at which a process should be woken
/// up.
///
/// The queue is effectively an extension of [`WakerSet`] ordered by wake time,
/// and is (currently) implemented as a pair of a `BTreeSet` and a `HashMap` to
/// allow efficient insertion and deduplication of wakers. See the documentation
/// of [`WakerSet`] for more details on the data structure of wakers and the
/// rationale behind it.
///
/// Like [`WakerSet`], wakers in this queue are compared by their pointer
/// addresses, and dead wakers in the queue may be automatically removed as a
/// side effect of other operations.
///
/// [`WakerSet`]: super::WakerSet
#[derive(Clone, Debug, Default)]
pub struct ScheduledWakerQueue {
    /// Set of scheduled wakers ordered by wake time
    wakers_by_time: BTreeSet<(Instant, WakerEntry)>,
    /// Map from wakers to their scheduled wake times for efficient deduplication
    waker_to_time: HashMap<WakerEntry, Instant>,
}

impl ScheduledWakerQueue {
    /// Creates a new empty `ScheduledWakerQueue`.
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of scheduled wakers in the queue.
    ///
    /// Wakers may become dead over time, so the actual number of valid wakers
    /// may be less than this count.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.wakers_by_time.len()
    }

    /// Checks if the queue is empty.
    ///
    /// Wakers may become dead over time, so there may be no valid wakers even
    /// if this method returns `false`.
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.wakers_by_time.is_empty()
    }

    /// Clears all scheduled wakers from the queue.
    pub fn clear(&mut self) {
        self.wakers_by_time.clear();
        self.waker_to_time.clear();
        self.validate();
    }

    /// Pushes a new scheduled waker into the queue.
    ///
    /// This method adds a new scheduled waker to the priority queue so that the
    /// associated process can be woken up at the specified time.
    ///
    /// Returns `true` if the waker was successfully added to the queue, or
    /// `false` if it was already present or dead, in which case the passed weak
    /// reference is dropped.
    ///
    /// The amortized time complexity of this method is O(log(n)). If the queue is
    /// full, it will first clean up dead wakers and possibly reallocate to
    /// optimize the capacity for future insertions, which will cost O(n log(n))
    /// time. Because of this cleanup, the number of wakers in the queue may
    /// decrease after calling this method, regardless of whether the new waker
    /// was added or not.
    pub fn push(&mut self, wake_time: Instant, waker: Weak<Cell<Option<Waker>>>) -> bool {
        let waker_entry = WakerEntry(waker);
        if !waker_entry.is_alive() {
            return false;
        }

        // Do some cleanup before insertion.
        self.trim_to_next_wake_time();
        if self.len() == self.waker_to_time.capacity() {
            // The hash map is full. Before it increases its capacity, we try to
            // clean up dead wakers from the queue to make room for new entries.
            self.wakers_by_time.retain(|(_wake_time, waker_entry)| {
                if waker_entry.is_alive() {
                    true
                } else {
                    self.waker_to_time.remove(waker_entry);
                    false
                }
            });

            // If we have removed substantially many wakers, we can also shrink
            // the capacity to save memory. This is not strictly necessary, but
            // it can help prevent the hash map from growing too large if many
            // wakers are added and removed over time.
            self.waker_to_time
                .shrink_to(std::cmp::max(8, self.len() * 2));

            // For amortized O(n log(n)) time complexity, we make sure the next
            // cleanup will not occur until the number of wakers doubles again.
            self.waker_to_time.reserve(self.len());
        }

        // Now we can insert the new waker.
        let pushed = match self.waker_to_time.get_mut(&waker_entry) {
            None => {
                self.waker_to_time.insert(waker_entry.clone(), wake_time);
                self.wakers_by_time.insert((wake_time, waker_entry))
            }
            Some(wake_time_entry) => {
                if *wake_time_entry <= wake_time {
                    // The existing entry is earlier than the new one,
                    // so we ignore the new entry
                    false
                } else {
                    // The new entry is earlier than the existing one,
                    // so we replace the existing entry with the new one
                    let old_wake_time = std::mem::replace(wake_time_entry, wake_time);
                    let waker_entry = self
                        .wakers_by_time
                        .take(&(old_wake_time, waker_entry))
                        .unwrap()
                        .1;
                    self.wakers_by_time.insert((wake_time, waker_entry))
                }
            }
        };

        self.validate();
        pushed
    }

    /// Returns the next scheduled wake time, if any.
    ///
    /// This method peeks at the priority queue to find the scheduled waker with
    /// the earliest wake time. If the queue is not empty, it returns the wake
    /// time of that waker; otherwise, it returns `None`.
    ///
    /// If you have a mutable reference to the queue, you can use
    /// [`trim_to_next_wake_time`](Self::trim_to_next_wake_time) instead of this
    /// method to remove dead wakers as the queue is traversed to find the next
    /// wake time.
    pub fn next_wake_time(&self) -> Option<Instant> {
        self.wakers_by_time
            .iter()
            .find(|&(_, entry)| entry.is_alive())
            .map(|(wake_time, _)| *wake_time)
    }

    /// Trims dead wakers to find the next wake time.
    ///
    /// This method removes dead wakers from the beginning of the priority queue
    /// until it finds a live waker or the queue becomes empty. The return value
    /// is the wake time of the first live waker in the queue after trimming, or
    /// `None` if the queue is empty.
    ///
    /// This method is solely for optimization purposes and does not affect the
    /// correctness of the queue. Using this method instead of
    /// [`next_wake_time`](Self::next_wake_time) can help avoid unnecessary
    /// processing of dead wakers, particularly when `next_wake_time` is
    /// followed by [`wake`](Self::wake) that will remove dead wakers anyway.
    ///
    /// The [`push`](Self::push) method will also call this method to clean up
    /// dead wakers before inserting a new waker, so you don't need to call this
    /// method manually in most cases.
    pub fn trim_to_next_wake_time(&mut self) -> Option<Instant> {
        let mut next_wake_time = None;
        while let Some((wake_time, waker_entry)) = self.wakers_by_time.first() {
            if waker_entry.is_alive() {
                next_wake_time = Some(*wake_time);
                break;
            }
            self.waker_to_time.remove(waker_entry);
            self.wakers_by_time.pop_first();
        }
        self.validate();
        next_wake_time
    }

    /// Wakes up processes whose scheduled wake time has been reached.
    ///
    /// This method checks the priority queue for any scheduled wakers whose
    /// wake time is less than or equal to the current time (`now`). For each
    /// such waker, it takes the waker from the `Cell` and calls `wake()` on it
    /// to wake up the associated process. After waking up the process, the item
    /// is removed from the queue.
    pub fn wake(&mut self, now: Instant) {
        while let Some((wake_time, waker_entry)) = self.wakers_by_time.first() {
            if *wake_time > now {
                break;
            }
            self.waker_to_time.remove(waker_entry);
            let waker_entry = self.wakers_by_time.pop_first().unwrap().1;
            if let Some(waker) = waker_entry.0.upgrade().and_then(|cell| cell.take()) {
                waker.wake();
            }
        }
        self.validate();
    }

    /// Validates the internal consistency of the queue.
    #[cfg(debug_assertions)]
    fn validate(&self) {
        assert_eq!(self.wakers_by_time.len(), self.waker_to_time.len());
        for (wake_time, entry) in &self.wakers_by_time {
            assert_eq!(*wake_time, self.waker_to_time[entry]);
        }
        for (entry, wake_time) in &self.waker_to_time {
            assert!(self.wakers_by_time.contains(&(*wake_time, entry.clone())));
        }
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
    fn queue_is_initially_empty() {
        let queue = ScheduledWakerQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn pushed_wakers_are_stored_in_queue() {
        let mut queue = ScheduledWakerQueue::new();
        let waker = dummy_waker();

        let pushed = queue.push(Instant::now(), Rc::downgrade(&waker));
        assert!(pushed);
        assert!(!queue.is_empty());
        assert_eq!(queue.len(), 1);
        assert_eq!(Rc::strong_count(&waker), 1);
        assert_eq!(Rc::weak_count(&waker), 2);

        let another_waker = dummy_waker();

        let pushed = queue.push(Instant::now(), Rc::downgrade(&another_waker));
        assert!(pushed);
        assert!(!queue.is_empty());
        assert_eq!(queue.len(), 2);
        assert_eq!(Rc::strong_count(&another_waker), 1);
        assert_eq!(Rc::weak_count(&another_waker), 2);
    }

    #[test]
    fn queue_is_empty_after_cleared() {
        let mut queue = ScheduledWakerQueue::new();
        let waker = dummy_waker();
        queue.push(Instant::now(), Rc::downgrade(&waker));

        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn pushing_existing_waker_with_earlier_wake_time_discards_existing_waker() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();
        let waker = dummy_waker();
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker));

        let pushed = queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker));
        assert!(pushed);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));
    }

    #[test]
    fn pushing_existing_waker_with_later_wake_time_discards_new_waker() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();
        let waker = dummy_waker();
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker));

        let pushed = queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker));
        assert!(!pushed);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));
    }

    #[test]
    fn pushing_dead_waker_is_noop() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let pushed = queue.push(now, Weak::new());
        assert!(!pushed);
        assert!(queue.is_empty());

        let waker = dummy_waker();
        waker.take();
        let pushed = queue.push(now, Rc::downgrade(&waker));
        assert!(!pushed);
        assert!(queue.is_empty());
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
        let waker_1 = dummy_waker();
        let waker_2 = dummy_waker();
        let waker_3 = dummy_waker();

        assert_eq!(queue.next_wake_time(), None);

        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_1));
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(5)));

        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_2));
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));

        queue.push(now + Duration::from_secs(10), Rc::downgrade(&waker_3));
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(3)));
    }

    #[test]
    fn next_wake_time_ignores_dead_wakers() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();
        let waker_1 = dummy_waker();
        let waker_2 = dummy_waker();
        let waker_3 = dummy_waker();
        queue.push(now, Rc::downgrade(&waker_1));
        queue.push(now, Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_3));
        drop(waker_1);
        waker_2.take();

        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(5)));
    }

    #[test]
    fn trim_to_next_wake_time_removes_leading_dead_wakers() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();
        let waker_1 = dummy_waker();
        let waker_2 = dummy_waker();
        let waker_3 = dummy_waker();
        queue.push(now, Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_3));
        drop(waker_1);
        waker_2.take();

        queue.trim_to_next_wake_time();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.next_wake_time(), Some(now + Duration::from_secs(5)));
    }

    #[test]
    fn trim_to_next_wake_time_returns_next_wake_time() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();
        let waker_1 = dummy_waker();
        let waker_2 = dummy_waker();
        let waker_3 = dummy_waker();
        queue.push(now, Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_3));
        drop(waker_1);
        waker_2.take();

        let next_wake_time = queue.trim_to_next_wake_time();
        assert_eq!(next_wake_time, Some(now + Duration::from_secs(5)));

        drop(waker_3);
        let next_wake_time = queue.trim_to_next_wake_time();
        assert_eq!(next_wake_time, None);
    }

    #[test]
    fn wake_removes_all_wakers_up_to_given_time() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let waker_1 = dummy_waker();
        let waker_2 = dummy_waker();
        let waker_3 = dummy_waker();
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(6), Rc::downgrade(&waker_3));

        // The first two wakers should be removed, but not the third one
        queue.wake(now + Duration::from_secs(5));
        assert_eq!(queue.len(), 1);
        assert_eq!(Rc::weak_count(&waker_1), 0);
        assert_eq!(Rc::weak_count(&waker_2), 0);
        assert_eq!(Rc::weak_count(&waker_3), 2);
    }

    #[test]
    fn wake_activates_all_wakers_up_to_given_time() {
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
    fn push_trims_earliest_dead_entries() {
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

        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_1));
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_2));
        queue.push(now + Duration::from_secs(7), Rc::downgrade(&waker_3));

        // Manually wake the first two wakers to simulate them being woken by other conditions
        waker_1.take().unwrap().wake();
        waker_2.take().unwrap().wake();

        // Now push a new waker, which should trigger the cleanup of the dead entries
        queue.push(now + Duration::from_secs(1), Rc::downgrade(&waker_4));
        assert_eq!(queue.len(), 2);

        // The remaining wakers should be the third and fourth ones
        queue.wake(now + Duration::from_secs(10));
        assert!(wake_flag_3.is_woken());
        assert!(wake_flag_4.is_woken());
    }

    #[test]
    fn push_cleans_up_all_dead_entries_if_full() {
        let mut queue = ScheduledWakerQueue::new();
        let now = Instant::now();

        let waker_1 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_2 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_3 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_4 = Rc::new(Cell::new(Some(Waker::noop().clone())));

        queue.waker_to_time.reserve(10);
        queue.push(now + Duration::from_secs(3), Rc::downgrade(&waker_1));
        while queue.len() + 1 < queue.waker_to_time.capacity() {
            let waker = dummy_waker();
            queue.push(
                now + Duration::new(3, queue.len() as u32),
                Rc::downgrade(&waker),
            );
        }
        queue.push(now + Duration::from_secs(4), Rc::downgrade(&waker_2));
        assert_eq!(queue.len(), queue.waker_to_time.capacity());

        // The next push should trigger cleanup of expired entries
        queue.push(now + Duration::from_secs(5), Rc::downgrade(&waker_3));
        assert_eq!(queue.len(), 3);

        // Manually wake the last waker to simulate it being woken by other conditions
        waker_3.take().unwrap().wake();

        // Another push does not trigger cleanup since the capacity is not yet reached
        queue.push(now + Duration::from_secs(6), Rc::downgrade(&waker_4));
        assert_eq!(queue.len(), 4);
    }
}
