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

//! Simple [`Waker`] implementation that only has a flag to indicate whether it
//! has been woken up or not

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Wake;
#[cfg(doc)]
use std::task::Waker;

/// Simple flag that indicates whether a [`Waker`] has been woken up or not
///
/// This type implements [`Wake`] and can be used to create a [`Waker`].
/// When the `Waker` is woken up, the flag will be set to `true`.
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::Ordering;
/// use std::task::Waker;
/// use yash_env::test_helper::WakeFlag;
///
/// let wake_flag = Arc::new(WakeFlag::default());
/// let waker = Waker::from(wake_flag.clone());
/// assert!(!wake_flag.is_woken());
/// waker.wake();
/// assert!(wake_flag.is_woken());
/// ```
#[derive(Debug, Default)]
#[repr(transparent)]
pub struct WakeFlag(pub AtomicBool);

impl WakeFlag {
    /// Creates a new `WakeFlag` with the flag set to `false`.
    #[inline(always)]
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Tests the flag, that is, whether the `WakeFlag` has been woken up or not.
    pub fn is_woken(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Wake for WakeFlag {
    #[inline(always)]
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl From<AtomicBool> for WakeFlag {
    #[inline(always)]
    fn from(value: AtomicBool) -> Self {
        Self(value)
    }
}

impl From<WakeFlag> for AtomicBool {
    #[inline(always)]
    fn from(value: WakeFlag) -> Self {
        value.0
    }
}
