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

//! File descriptor set for the virtual system
//!
//! This module defines the [`FdSet` struct](FdSet), which is an implementation
//! of the [`FdSet` trait](FdSetTrait) for the
//! [`VirtualSystem`](super::VirtualSystem). The module also provides iterators
//! for iterating over the file descriptors in an `FdSet`.

use crate::io::{Fd, RawFd};
use crate::system::FdSet as FdSetTrait;
use std::collections::BTreeSet;

/// File descriptor set for the virtual system
///
/// This is an implementation of the [`FdSet` trait](FdSetTrait) for the
/// [`VirtualSystem`](super::VirtualSystem). It represents a set of file
/// descriptors that can be monitored for events such as readability or
/// writability in the virtual system. Currently, the `FdSet` struct internally
/// uses a `BTreeSet` to store the file descriptors, which allows for efficient
/// insertion, removal, and lookup of file descriptors.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FdSet(BTreeSet<Fd>);

impl FdSetTrait for FdSet {
    const MAX_FD: Fd = Fd(RawFd::MAX);

    #[inline(always)]
    fn insert(&mut self, fd: Fd) {
        if fd.0 >= 0 {
            self.0.insert(fd);
        }
    }

    #[inline(always)]
    fn remove(&mut self, fd: Fd) {
        self.0.remove(&fd);
    }

    #[inline(always)]
    fn contains(&self, fd: Fd) -> bool {
        self.0.contains(&fd)
    }
}

impl From<Fd> for FdSet {
    #[inline(always)]
    fn from(fd: Fd) -> Self {
        let mut set = Self::new();
        set.insert(fd);
        set
    }
}

impl FromIterator<Fd> for FdSet {
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = Fd>>(iter: I) -> Self {
        Self(FromIterator::from_iter(iter))
    }
}

/// Iterator over the file descriptors in an `FdSet`
///
/// Use `FdSet::into_iter` to create an iterator from an `FdSet`.
#[derive(Debug)]
pub struct IntoIter(std::collections::btree_set::IntoIter<Fd>);

impl Iterator for IntoIter {
    type Item = Fd;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl IntoIterator for FdSet {
    type Item = Fd;
    type IntoIter = IntoIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

/// Iterator over the file descriptors in an `FdSet` by reference
///
/// Use `FdSet::iter` to create an iterator from a reference to an `FdSet`.
#[derive(Debug)]
pub struct Iter<'a>(std::collections::btree_set::Iter<'a, Fd>);

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Fd;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a> IntoIterator for &'a FdSet {
    type Item = &'a Fd;
    type IntoIter = Iter<'a>;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        Iter(self.0.iter())
    }
}

impl Extend<Fd> for FdSet {
    fn extend<I: IntoIterator<Item = Fd>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<'a> Extend<&'a Fd> for FdSet {
    fn extend<I: IntoIterator<Item = &'a Fd>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl FdSet {
    /// Returns the number of file descriptors in the set.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the set is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the file descriptors in the set.
    #[inline(always)]
    pub fn iter(&self) -> Iter<'_> {
        self.into_iter()
    }
}
