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

//! Signal-related functionality for the system module

use super::Result;
#[cfg(doc)]
use super::SharedSystem;
pub use crate::signal::{Name, Number, RawNumber};

/// Trait for managing available signals
pub trait Signals {
    /// Tests if a signal number is valid.
    ///
    /// This function returns `Some((name, number))` if the signal number refers
    /// to a valid signal supported by the system. Otherwise, it returns `None`.
    ///
    /// Note that one signal number can have multiple names, in which case this
    /// function returns the name that is considered the most common.
    #[must_use]
    fn validate_signal(&self, number: RawNumber) -> Option<(Name, Number)>;

    /// Gets the signal number from the signal name.
    ///
    /// This function returns the signal number corresponding to the signal name
    /// in the system. If the signal name is not supported, it returns `None`.
    #[must_use]
    fn signal_number_from_name(&self, name: Name) -> Option<Number>;
}

/// Operation applied to the signal blocking mask
///
/// This enum corresponds to the operations of the `sigprocmask` system call and
/// is used in the [`Sigmask::sigmask`] method.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SigmaskOp {
    /// Add signals to the mask (`SIG_BLOCK`)
    Add,
    /// Remove signals from the mask (`SIG_UNBLOCK`)
    Remove,
    /// Set the mask to the given signals (`SIG_SETMASK`)
    Set,
}

/// Trait for managing signal blocking mask
pub trait Sigmask {
    /// Gets and/or sets the signal blocking mask.
    ///
    /// This is a low-level function used internally by [`SharedSystem`]. You
    /// should not call this function directly, or you will disrupt the behavior
    /// of `SharedSystem`. The description below applies if you want to do
    /// everything yourself without depending on `SharedSystem`.
    ///
    /// This is a thin wrapper around the [`sigprocmask` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/pthread_sigmask.html).
    /// If `op` is `Some`, this function updates the signal blocking mask by
    /// applying the given `SigmaskOp` and signal set to the current mask. If
    /// `op` is `None`, this function does not change the mask.
    /// If `old_mask` is `Some`, this function sets the previous mask to it.
    fn sigmask(
        &mut self,
        op: Option<(SigmaskOp, &[Number])>,
        old_mask: Option<&mut Vec<Number>>,
    ) -> Result<()>;
}

/// How the shell process responds to a signal
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Disposition {
    /// Perform the default action for the signal.
    ///
    /// The default action depends on the signal. For example, `SIGINT` causes
    /// the process to terminate, and `SIGTSTP` causes the process to stop.
    #[default]
    Default,
    /// Ignore the signal.
    Ignore,
    /// Catch the signal.
    Catch,
}

/// Trait for managing signal dispositions
pub trait Sigaction {
    /// Gets the disposition for a signal.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem`]. You should not call this function directly, or you
    /// will leave the `SharedSystem` instance in an inconsistent state. The
    /// description below applies if you want to do everything yourself without
    /// depending on `SharedSystem`.
    ///
    /// This is an abstract wrapper around the [`sigaction` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/sigaction.html).
    /// This function returns the current disposition if successful.
    ///
    /// To change the disposition, use [`sigaction`](Self::sigaction).
    fn get_sigaction(&self, signal: Number) -> Result<Disposition>;

    /// Gets and sets the disposition for a signal.
    ///
    /// This is a low-level function used internally by [`SharedSystem`]. You
    /// should not call this function directly, or you will leave the
    /// `SharedSystem` instance in an inconsistent state. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// This is an abstract wrapper around the [`sigaction` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/sigaction.html).
    /// This function returns the previous disposition if successful.
    ///
    /// When you set the disposition to `Disposition::Catch`, signals sent to
    /// this process are accumulated in `self` and made available from
    /// [`caught_signals`](super::System::caught_signals).
    ///
    /// To get the current disposition without changing it, use
    /// [`get_sigaction`](Self::get_sigaction).
    fn sigaction(&mut self, signal: Number, action: Disposition) -> Result<Disposition>;
}
