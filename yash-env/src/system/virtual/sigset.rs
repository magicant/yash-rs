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

//! Signal set operations for the virtual system
//!
//! This module defines the [`Sigset` struct](Sigset), which is an implementation of the
//! [`Sigset` trait](SigsetTrait) for the [`VirtualSystem`]. The module also provides
//! iterators for iterating over the signals in a `Sigset`.

use super::RT_RANGE;
use super::VirtualSystem;
use crate::signal::Number;
use crate::system::{Result, Signals as _, Sigset as SigsetTrait};
use std::collections::BTreeSet;
use std::num::NonZero;

/// Set of signal numbers
///
/// This struct is an implementation of the [`Sigset` trait](SigsetTrait) for the
/// [`VirtualSystem`]. It represents a set of signal numbers that can be used
/// for signal blocking and other signal-related operations in the virtual
/// system. Currently, the `Sigset` struct internally uses a `BTreeSet` to store
/// the signal numbers, which allows for efficient insertion, removal, and
/// lookup of signals.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct Sigset(BTreeSet<Number>);

impl SigsetTrait for Sigset {
    /// Creates a new set containing all the signals supported by the virtual system.
    fn full() -> Self {
        let named = VirtualSystem::NAMED_SIGNALS.iter().filter_map(|(_, n)| *n);
        let realtime = RT_RANGE.map(|num| Number::from_raw_unchecked(NonZero::new(num).unwrap()));
        Self(BTreeSet::from_iter(named.chain(realtime)))
    }

    /// Adds the specified signal to the set.
    ///
    /// The current implementation does not check whether the given signal is
    /// valid or not. The future implementation may return an error if the
    /// signal is invalid.
    fn insert(&mut self, signal: Number) -> Result<()> {
        self.0.insert(signal);
        Ok(())
    }

    /// Removes the specified signal from the set.
    ///
    /// The current implementation does not check whether the given signal is
    /// valid or not. The future implementation may return an error if the
    /// signal is invalid.
    fn remove(&mut self, signal: Number) -> Result<()> {
        self.0.remove(&signal);
        Ok(())
    }

    /// Checks whether the specified signal is in the set or not.
    ///
    /// The current implementation does not check whether the given signal is
    /// valid or not. The future implementation may return an error if the
    /// signal is invalid.
    fn contains(&self, signal: Number) -> Result<bool> {
        Ok(self.0.contains(&signal))
    }

    /// Creates a new set containing the signals in the given iterator.
    ///
    /// The current implementation does not check whether the given signals are
    /// valid or not. The future implementation may return an error if any of
    /// the signals is invalid.
    fn from_signals<I>(iter: I) -> Result<Self>
    where
        I: IntoIterator<Item = Number>,
    {
        Ok(Self(BTreeSet::from_iter(iter)))
    }
}

impl From<Number> for Sigset {
    /// Creates a new set containing the specified signal.
    ///
    /// The current implementation does not check whether the given signal is
    /// valid or not. The future implementation may return an empty set if the
    /// signal is invalid.
    fn from(signal: Number) -> Self {
        let mut set = Sigset::default();
        set.0.insert(signal);
        set
    }
}

impl FromIterator<Number> for Sigset {
    /// Creates a new set containing the signals in the given iterator.
    ///
    /// The current implementation does not check whether the given signals are
    /// valid or not. The future implementation may silently ignore any invalid
    /// signal.
    fn from_iter<T: IntoIterator<Item = Number>>(iter: T) -> Self {
        Self(BTreeSet::from_iter(iter))
    }
}

/// Iterator over the signals in a [`Sigset`]
///
/// Use `Sigset::into_iter` to create an iterator over a `Sigset`.
#[derive(Debug)]
pub struct IntoIter(std::collections::btree_set::IntoIter<Number>);

impl Iterator for IntoIter {
    type Item = Number;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl IntoIterator for Sigset {
    type Item = Number;
    type IntoIter = IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

/// Iterator over the signals in a [`Sigset`] reference
///
/// Use `Sigset::iter` to create an iterator over a `Sigset`.
#[derive(Debug)]
pub struct Iter<'a>(std::collections::btree_set::Iter<'a, Number>);

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Number;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a> IntoIterator for &'a Sigset {
    type Item = &'a Number;
    type IntoIter = Iter<'a>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        Iter(self.0.iter())
    }
}

impl Extend<Number> for Sigset {
    #[inline(always)]
    fn extend<I: IntoIterator<Item = Number>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<'a> Extend<&'a Number> for Sigset {
    #[inline(always)]
    fn extend<I: IntoIterator<Item = &'a Number>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl Sigset {
    /// Returns the number of signals in the set.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the set is empty or not.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the signals in the set.
    #[inline(always)]
    pub fn iter(&self) -> Iter<'_> {
        self.into_iter()
    }

    pub(super) fn difference_to_vec(&self, other: &Sigset) -> Vec<Number> {
        self.0.difference(&other.0).copied().collect()
    }
}
