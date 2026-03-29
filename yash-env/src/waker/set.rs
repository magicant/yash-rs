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

//! Items for managing collections of wakers in the virtual system

use super::WakerEntry;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Weak;
use std::task::Waker;

/// Set of wakers to be awakened when a certain event occurs
///
/// A `WakerSet` stores wakers that can later be awakened. It is used in the
/// virtual system to manage wakers for events such as file readiness and signal
/// delivery.
///
/// Wakers are stored as `Weak<Cell<Option<Waker>>>`. The asynchronous task that
/// wants to be awakened creates a `Cell<Option<Waker>>` to hold its waker, and
/// keeps a `Rc` to it until it is awakened as expected or it wants to cancel
/// the wake-up. If the strong reference is dropped and the cell is deallocated
/// before the waker is awakened, the `WakerSet` will not attempt to wake it, as
/// the `Weak` reference will fail to upgrade. This design allows for automatic
/// cleanup of wakers that are no longer valid without requiring explicit
/// removal from the set.
///
/// The cell of the optional waker allows taking the waker out of the cell when
/// waking, which is necessary to call `Waker::wake` without the need to clone
/// the waker. After waking, the cell is set to `None` to indicate that the
/// waker has been consumed and should not be woken again. This design also
/// allows multiple sets for different events to share the same waker cell, as
/// waking from one set will consume the waker and prevent it from being woken
/// again from other sets, which is suitable when a task is waiting for multiple
/// events and should be awakened when any of them occurs.
///
/// Wakers are considered **dead** when their weak reference cannot be upgraded
/// or their cell contains `None`. Dead wakers are not awakened and may be
/// automatically removed from the set as a side effect of other set operations.
///
/// A `WakerSet` internally uses a `HashSet` to store wakers. Wakers are
/// compared by their pointer addresses, so adding the same waker multiple times
/// will not create duplicates in the set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WakerSet {
    wakers: HashSet<WakerEntry>,
}

impl WakerSet {
    /// Creates a new empty `WakerSet`.
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the capacity of the set.
    #[inline(always)]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.wakers.capacity()
    }

    /// Reserves capacity for at least `additional` more wakers to be inserted
    /// without reallocating.
    ///
    /// After calling this method, the capacity will be greater than or equal to
    /// the current item count plus `additional`. Does nothing if the capacity
    /// is already sufficient.
    #[inline(always)]
    pub fn reserve(&mut self, additional: usize) {
        self.wakers.reserve(additional)
    }

    /// Shrinks the capacity of the set as much as possible.
    #[inline(always)]
    pub fn shrink_to_fit(&mut self) {
        self.wakers.shrink_to_fit()
    }

    /// Shrinks the capacity of the set to the specified minimum.
    ///
    /// After this operation, the capacity will be at least `min_capacity`, but
    /// the set may have more capacity than that.
    #[inline(always)]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.wakers.shrink_to(min_capacity)
    }

    /// Returns the number of wakers in the set.
    ///
    /// Wakers may become dead over time, so the actual number of valid wakers
    /// may be less than this count.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.wakers.len()
    }

    /// Returns `true` if the set contains no wakers.
    ///
    /// Wakers may become dead over time, so there may be no valid wakers even
    /// if this method returns `false`.
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.wakers.is_empty()
    }

    /// Inserts a waker into the set.
    ///
    /// Returns `true` if the waker was not already present in the set, and
    /// `false` if it was already present, in which case the set is not modified
    /// and the passed weak reference is dropped.
    ///
    /// The amortized time complexity of this method is O(1). If the set is
    /// full, it will first clean up dead wakers and possibly reallocate to
    /// optimize the capacity for future insertions, which will cost O(n) time.
    #[inline]
    pub fn insert(&mut self, waker_cell: Weak<Cell<Option<Waker>>>) -> bool {
        if self.len() == self.capacity() {
            // If the set is full, the inner `HashSet` will need to reallocate
            // to insert a new waker, which will cost O(n) time. This is a good
            // opportunity to clean up dead wakers, which will also cost O(n)
            // time.
            self.wakers.retain(WakerEntry::is_alive);

            // If we have removed substantially many wakers, we can also shrink
            // the capacity to save memory. This is not strictly necessary, but
            // it can help prevent the set from growing too large if many wakers
            // are added and removed over time.
            self.shrink_to(std::cmp::max(8, self.wakers.len() * 2));

            // For amortized O(1) time complexity, we make sure the next cleanup
            // will not occur until the number of wakers doubles again.
            self.reserve(self.wakers.len());
        }

        self.wakers.insert(WakerEntry(waker_cell))
    }

    /// Wakes all wakers in the set and clears the set.
    ///
    /// If a waker has been consumed or its strong reference has been dropped,
    /// it is not awakened and simply removed from the set.
    pub fn wake_all(&mut self) {
        self.wakers
            .drain()
            .filter_map(|entry| entry.0.upgrade().and_then(|cell| cell.take()))
            .for_each(Waker::wake);
    }

    /// Clears the set without waking any wakers.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.wakers.clear()
    }
}

impl FromIterator<Weak<Cell<Option<Waker>>>> for WakerSet {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Weak<Cell<Option<Waker>>>>,
    {
        let wakers = HashSet::from_iter(iter.into_iter().map(WakerEntry));
        Self { wakers }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helper::WakeFlag;
    use std::rc::Rc;
    use std::sync::Arc;

    #[test]
    fn waking_all_inserted_wakers() {
        let mut set = WakerSet::new();
        let wake_flag_1 = Arc::new(WakeFlag::new());
        let wake_flag_2 = Arc::new(WakeFlag::new());
        let waker_1 = Rc::new(Cell::new(Some(Waker::from(wake_flag_1.clone()))));
        let waker_2 = Rc::new(Cell::new(Some(Waker::from(wake_flag_2.clone()))));
        assert!(set.insert(Rc::downgrade(&waker_1)));
        assert!(set.insert(Rc::downgrade(&waker_2)));

        set.wake_all();
        assert!(wake_flag_1.is_woken());
        assert!(wake_flag_2.is_woken());
        assert!(set.is_empty());
    }

    #[test]
    fn duplicate_wakers_are_not_inserted() {
        let mut set = WakerSet::new();
        let waker_1 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_1_clone = Rc::clone(&waker_1);
        let waker_2 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        assert!(set.insert(Rc::downgrade(&waker_1)));
        assert!(set.insert(Rc::downgrade(&waker_2)));
        assert_eq!(set.len(), 2);

        assert!(!set.insert(Rc::downgrade(&waker_1)));
        assert!(!set.insert(Rc::downgrade(&waker_1_clone)));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clearing_waker_set() {
        let mut set = WakerSet::new();
        let waker_1 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_2 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        assert!(set.insert(Rc::downgrade(&waker_1)));
        assert!(set.insert(Rc::downgrade(&waker_2)));
        assert_eq!(set.len(), 2);

        set.clear();
        assert!(set.is_empty());
        assert_eq!(Rc::weak_count(&waker_1), 0);
        assert_eq!(Rc::weak_count(&waker_2), 0);
    }

    #[test]
    fn dead_wakers_are_removed_before_insertion_if_full() {
        let mut set = WakerSet::new();
        let waker_1 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_2 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_3 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_4 = Rc::new(Cell::new(Some(Waker::noop().clone())));
        let waker_5 = Rc::new(Cell::new(Some(Waker::noop().clone())));

        set.reserve(10);
        assert!(set.insert(Rc::downgrade(&waker_1)));
        assert!(set.insert(Rc::downgrade(&waker_2)));
        waker_2.take(); // Consume waker_2 to make it dead
        assert!(set.insert(Rc::downgrade(&waker_3)));
        while set.len() < set.capacity() - 1 {
            let waker = Rc::new(Cell::new(Some(Waker::noop().clone())));
            assert!(set.insert(Rc::downgrade(&waker)));
        }
        assert!(set.insert(Rc::downgrade(&waker_4)));
        assert_eq!(set.len(), set.capacity());

        // Now the set is full. Inserting waker_5 should trigger cleanup of dead wakers.
        assert!(set.insert(Rc::downgrade(&waker_5)));
        assert_eq!(Rc::weak_count(&waker_1), 1);
        assert_eq!(Rc::weak_count(&waker_2), 0);
        assert_eq!(Rc::weak_count(&waker_3), 1);
        assert_eq!(Rc::weak_count(&waker_4), 1);
        assert_eq!(Rc::weak_count(&waker_5), 1);
        assert_eq!(set.len(), 4);
    }
}
