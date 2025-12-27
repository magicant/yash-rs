// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Items about time

use super::Result;
use std::time::Instant;

/// Trait for getting the current time
pub trait Time {
    /// Returns the current time.
    #[must_use]
    fn now(&self) -> Instant;
}

/// Set of consumed CPU time statistics
///
/// This structure contains four CPU time values, all in seconds.
///
/// This structure is returned by [`Times::times`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTimes {
    /// User CPU time consumed by the current process
    pub self_user: f64,
    /// System CPU time consumed by the current process
    pub self_system: f64,
    /// User CPU time consumed by the children of the current process
    pub children_user: f64,
    /// System CPU time consumed by the children of the current process
    pub children_system: f64,
}

/// Trait for getting consumed CPU time statistics
pub trait Times {
    /// Returns the consumed CPU time statistics.
    ///
    /// This function abstracts the behavior of the
    /// [`times` system call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/times.html).
    fn times(&self) -> Result<CpuTimes>;
}
