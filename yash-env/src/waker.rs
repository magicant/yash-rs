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

//! Utilities for managing [`Waker`]s
//!
//! This module contains some utilities for managing [`Waker`]s. These are
//! primarily used by the internal implementation of the [`VirtualSystem`].
//!
//! [`VirtualSystem`]: crate::system::virtual::VirtualSystem

mod queue;
mod set;

use std::cell::Cell;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::rc::Weak;
use std::task::Waker;

pub use self::queue::ScheduledWakerQueue;
pub use self::set::WakerSet;

/// Entry of [`WakerSet`] and [`ScheduledWakerQueue`]
///
/// This is the new type pattern applied to `Weak<Cell<Option<Waker>>>` to
/// implement `Eq`, `Hash`, and `Ord`. Wakers are compared by their pointer
/// addresses. We do not use [`Waker::will_wake`] because actual wakers are
/// stored in `Cell`s, which support interior mutability and thus may have their
/// wakers changed after being added to the set.
#[derive(Clone, Debug)]
struct WakerEntry(pub Weak<Cell<Option<Waker>>>);

impl PartialEq for WakerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for WakerEntry {}

impl Hash for WakerEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_ptr().addr().hash(state);
    }
}

impl PartialOrd for WakerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WakerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_ptr().addr().cmp(&other.0.as_ptr().addr())
    }
}

impl WakerEntry {
    /// Checks if a waker entry is alive (i.e., its weak reference can be
    /// upgraded and its cell contains a waker that has not been consumed).
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.0.upgrade().is_some_and(|cell| {
            // Since `Waker` is not `Copy`, we need to take it from the
            // `Cell` to check if it's `None`, and put it back afterward.
            let waker = cell.take();
            let is_alive = waker.is_some();
            cell.set(waker);
            is_alive
        })
    }
}
