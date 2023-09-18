// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! File descriptor set
//!
//! Items defined in this module are mainly used for arguments to [`select`].
//!
//! [`select`]: crate::system::System::select

use nix::errno::Errno;
use nix::libc;
use std::os::fd::RawFd;
use thiserror::Error;
use yash_syntax::syntax::Fd;

/// Error indicating that the file descriptor is invalid
///
/// This error occurs when a specified file descriptor is negative or too large
/// (`>= libc::FD_SETSIZE`).
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[error("invalid file descriptor")]
pub struct InvalidFd;

/// Converts `InvalidFd` to `Errno::EBADF`.
impl From<InvalidFd> for Errno {
    fn from(InvalidFd: InvalidFd) -> Errno {
        Errno::EBADF
    }
}

fn validate(fd: Fd) -> Result<RawFd, InvalidFd> {
    if (0..(libc::FD_SETSIZE as _)).contains(&fd.0) {
        Ok(fd.0)
    } else {
        Err(InvalidFd)
    }
}

/// Set of file descriptors
#[derive(Clone, Copy, Debug, Eq)]
pub struct FdSet {
    /// Inner set of file descriptors
    pub(crate) inner: libc::fd_set,

    /// Some file descriptor that is larger than any file descriptor in the set
    ///
    /// This value is used to optimize the iteration over the file descriptors
    /// in the set. Only file descriptors below this value are considered.
    upper_bound: Fd,
}

impl FdSet {
    /// Creates a new empty set.
    #[must_use]
    pub fn new() -> Self {
        let inner = unsafe {
            let mut inner = std::mem::MaybeUninit::uninit();
            libc::FD_ZERO(inner.as_mut_ptr());
            inner.assume_init()
        };
        let upper_bound = Fd(0);
        Self { inner, upper_bound }
    }

    /// Inserts a file descriptor into the set.
    pub fn insert(&mut self, fd: Fd) -> Result<(), InvalidFd> {
        let fd = validate(fd)?;
        unsafe { libc::FD_SET(fd, &mut self.inner) };
        self.upper_bound = self.upper_bound.max(Fd(fd + 1));
        Ok(())
    }

    /// Removes a file descriptor from the set.
    pub fn remove(&mut self, fd: Fd) {
        if let Ok(fd) = validate(fd) {
            unsafe { libc::FD_CLR(fd, &mut self.inner) }
        }
    }

    /// Removes all file descriptors from the set.
    pub fn clear(&mut self) {
        unsafe { libc::FD_ZERO(&mut self.inner) }
    }

    /// Returns `true` if the set contains the file descriptor.
    #[must_use]
    pub fn contains(&self, fd: Fd) -> bool {
        match validate(fd) {
            Ok(fd) => unsafe { libc::FD_ISSET(fd, &self.inner) },
            Err(_) => false,
        }
    }

    /// Returns some file descriptor that is larger than any file descriptor in the set.
    #[must_use]
    pub fn upper_bound(&self) -> Fd {
        self.upper_bound
    }

    /// Returns an iterator over the file descriptors in the set.
    pub fn iter(&self) -> Iter {
        Iter {
            fd_set: self,
            range: 0..self.upper_bound.0,
        }
    }
}

impl Default for FdSet {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for FdSet {
    fn eq(&self, other: &Self) -> bool {
        // The upper bound does not affect the equality
        self.inner == other.inner
    }
}

impl std::hash::Hash for FdSet {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // The upper bound does not affect the hash
        self.inner.hash(state)
    }
}

/// Iterator over the file descriptors in a set
#[derive(Debug)]
pub struct Iter<'a> {
    fd_set: &'a FdSet,
    range: std::ops::Range<RawFd>,
}

impl Iterator for Iter<'_> {
    type Item = Fd;

    fn next(&mut self) -> Option<Fd> {
        loop {
            let fd = Fd(self.range.next()?);
            if self.fd_set.contains(fd) {
                return Some(fd);
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Fd> {
        loop {
            let fd = Fd(self.range.next_back()?);
            if self.fd_set.contains(fd) {
                return Some(fd);
            }
        }
    }
}

impl std::iter::FusedIterator for Iter<'_> {}

impl<'a> IntoIterator for &'a FdSet {
    type Item = Fd;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_set_is_empty() {
        let set = FdSet::new();
        assert!(!set.contains(Fd::STDIN));
        assert!(!set.contains(Fd::STDOUT));
        assert!(!set.contains(Fd::STDERR));
        assert_eq!(set.iter().next(), None);
    }

    #[test]
    fn adding_fd_to_set() {
        let mut set = FdSet::new();
        set.insert(Fd::STDIN).unwrap();
        assert!(set.contains(Fd::STDIN));
    }

    #[test]
    fn adding_many_fds_to_set() {
        let mut set = FdSet::new();
        set.insert(Fd::STDOUT).unwrap();
        set.insert(Fd::STDERR).unwrap();
        set.insert(Fd(3)).unwrap();
        assert!(!set.contains(Fd::STDIN));
        assert!(set.contains(Fd::STDOUT));
        assert!(set.contains(Fd::STDERR));
        assert!(set.contains(Fd(3)));
    }

    #[test]
    fn adding_invalid_fd_to_set() {
        let mut set = FdSet::new();
        set.insert(Fd(-1)).unwrap_err();
        set.insert(Fd(libc::FD_SETSIZE as _)).unwrap_err();
    }

    #[test]
    fn removing_fd_from_set() {
        let mut set = FdSet::new();
        set.insert(Fd::STDIN).unwrap();

        set.remove(Fd::STDIN);
        assert!(!set.contains(Fd::STDIN));
    }

    #[test]
    fn clearing_set() {
        let mut set = FdSet::new();
        set.insert(Fd::STDOUT).unwrap();
        set.insert(Fd::STDERR).unwrap();

        set.clear();
        assert!(!set.contains(Fd::STDIN));
        assert!(!set.contains(Fd::STDOUT));
        assert!(!set.contains(Fd::STDERR));
        assert_eq!(set.iter().next(), None);
    }

    #[test]
    fn adding_fd_updates_upper_bound() {
        let mut set = FdSet::new();
        assert_eq!(set.upper_bound(), Fd(0));

        set.insert(Fd(1)).unwrap();
        assert_eq!(set.upper_bound(), Fd(2));

        set.insert(Fd(0)).unwrap();
        assert_eq!(set.upper_bound(), Fd(2));

        set.insert(Fd(2)).unwrap();
        assert_eq!(set.upper_bound(), Fd(3));

        set.remove(Fd(2));
        assert!(set.upper_bound() >= Fd(2), "{:?}", set.upper_bound());
    }

    #[test]
    fn equality_ignores_upper_bound() {
        let mut set = FdSet::new();
        assert_eq!(set, set);

        set.insert(Fd(1)).unwrap();
        set.insert(Fd(4)).unwrap();
        assert_eq!(set, set);

        let mut new_set = set;
        new_set.insert(Fd(5)).unwrap();
        assert_ne!(set, new_set);
        new_set.remove(Fd(5));
        assert_eq!(set, new_set);
    }

    #[test]
    fn iterating_fds() {
        let mut set = FdSet::new();
        set.insert(Fd(1)).unwrap();
        set.insert(Fd(6)).unwrap();
        set.insert(Fd(3)).unwrap();

        let fds = set.iter().collect::<Vec<_>>();
        assert_eq!(fds, [Fd(1), Fd(3), Fd(6)]);
    }

    #[test]
    fn reverse_iterating_fds() {
        let fd_max = Fd((libc::FD_SETSIZE - 1) as _);

        let mut set = FdSet::new();
        set.insert(Fd(0)).unwrap();
        set.insert(fd_max).unwrap();

        let fds = set.iter().rev().collect::<Vec<_>>();
        assert_eq!(fds, [fd_max, Fd(0)]);
    }
}
