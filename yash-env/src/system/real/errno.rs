// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Extensions to [`Errno`] that depends on the real system

use super::super::Errno;

impl Errno {
    /// Returns the current `errno` value.
    ///
    /// This function returns an `Errno` value containing the current `errno`
    /// value, which is the error value of the last system call. Note that
    /// this function should be called immediately after a system call that
    /// sets `errno`, because the value of `errno` may be changed by other
    /// system calls whether or not they succeed.
    #[inline]
    #[must_use]
    pub(super) fn last() -> Self {
        Self(nix::Error::last() as _)
    }

    // TODO Need nix 0.28.0
    // /// Sets the current `errno` value.
    // ///
    // /// This function sets the current `errno` value to the specified value.
    // /// The next call to [`last`](Self::last) will return the specified value
    // /// unless another system call changes the `errno` value. This function is
    // /// useful when you want to simulate an error condition in a system call.
    // ///
    // /// Use [`clear`](Self::clear) to reset the `errno` value.
    // pub(super) fn set_last(errno: Self) {
    //     nix::Error::set_raw(errno.0)
    // }

    /// Clears the current `errno` value.
    ///
    /// Some platform functions do not indicate errors in their return values,
    /// and set the `errno` value only when an error occurs. In such cases, it
    /// is necessary to clear the `errno` value before calling the function
    /// and check the `errno` value after calling the function to see if an
    /// error occurred. This function resets the current `errno` value to
    /// [`NO_ERROR`](Self::NO_ERROR).
    // ///
    // /// Use [`set_last`](Self::set_last) to set the `errno` value to an
    // /// arbitrary value.
    #[inline]
    pub(super) fn clear() {
        // Self::set_last(Self::NO_ERROR)
        nix::Error::clear()
    }
}
