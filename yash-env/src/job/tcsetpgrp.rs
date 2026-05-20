// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Items related to the `tcsetpgrp` operation

use super::Pid;
use crate::io::Fd;
use crate::signal;
#[cfg(doc)]
use crate::system::Concurrent;
use crate::system::{
    Disposition, Result, Sigaction, Sigmask, SigmaskOp, Signals, Sigset as _, TcSetPgrp,
};

/// A trait to run a function with a signal blocked
///
/// This trait represents the capability required by [`tcsetpgrp_with_block`] to
/// run a function with a signal blocked. It is automatically implemented for
/// any type that implements [`Sigmask`]. Additionally, [`Concurrent`]
/// implements this trait by delegating to the inner type while not implementing
/// [`Sigmask`] itself, which allows `Concurrent` to maintain internal
/// consistency about signal masks while still providing this capability.
///
/// This trait defines a higher-level interface to temporarily modify the signal
/// mask. Typical implementations of this trait will internally depend on
/// [`Sigmask`] to perform the actual signal mask modification, but the trait
/// itself does not require this as a supertrait.
pub trait RunBlocking: Signals {
    /// Runs the given function with the specified signal blocked.
    ///
    /// This function blocks the given signal, runs the given function, and then
    /// restores the original signal mask. If all operations succeed, the result
    /// of the function is returned. If any operation fails, an error is
    /// returned.
    ///
    /// This function restores the original signal mask even if the given
    /// function returns an error, in which case any error restoring the signal
    /// mask is discarded. If the signal cannot be blocked, this function
    /// returns an error without running the function.
    fn run_blocking<F, T>(
        &self,
        signal: signal::Number,
        f: F,
    ) -> impl Future<Output = Result<T>> + use<'_, Self, F, T>
    where
        F: AsyncFnOnce() -> Result<T>;
}

impl<S> RunBlocking for S
where
    S: Sigmask + ?Sized,
{
    async fn run_blocking<F, T>(&self, signal: signal::Number, f: F) -> Result<T>
    where
        F: AsyncFnOnce() -> Result<T>,
    {
        let mut old_mask = S::Sigset::new();
        self.sigmask(
            Some((SigmaskOp::Add, &S::Sigset::from_signals([signal])?)),
            Some(&mut old_mask),
        )
        .await?;

        let main_result = f().await;

        let restore_result = self.sigmask(Some((SigmaskOp::Set, &old_mask)), None).await;
        if main_result.is_ok() {
            restore_result?;
        }

        main_result
    }
}

/// Switches the foreground process group with SIGTTOU blocked.
///
/// This is a convenience function to change the foreground process group
/// safely. If you call [`TcSetPgrp::tcsetpgrp`] from a background process, the
/// process is stopped by SIGTTOU by default. To prevent this effect, SIGTTOU
/// must be blocked or ignored when `tcsetpgrp` is called. This function uses
/// [`RunBlocking::run_blocking`] to block SIGTTOU while calling `tcsetpgrp`,
/// which ensures that the shell is not suspended even if it is not in the
/// foreground.
///
/// Use [`tcsetpgrp_without_block`] if you need to make sure the shell is in the
/// foreground before changing the foreground job.
pub async fn tcsetpgrp_with_block<S>(system: &S, fd: Fd, pgid: Pid) -> Result<()>
where
    S: RunBlocking + TcSetPgrp + ?Sized,
{
    system
        .run_blocking(S::SIGTTOU, || system.tcsetpgrp(fd, pgid))
        .await
}

/// A trait to run a function with a signal unblocked and the default disposition
///
/// This trait represents the capability required by [`tcsetpgrp_without_block`]
/// to run a function with a signal unblocked and the default disposition. It is
/// automatically implemented for any type that implements [`Sigmask`] and
/// [`Sigaction`]. Additionally, [`Concurrent`] implements this trait by
/// delegating to the inner type while not implementing [`Sigmask`] or
/// [`Sigaction`] itself, which allows `Concurrent` to maintain internal
/// consistency about signal masks and dispositions while still providing this
/// capability.
///
/// This trait defines a higher-level interface to temporarily modify the signal
/// mask and disposition. Typical implementations of this trait will internally
/// depend on [`Sigmask`] and [`Sigaction`] to perform the actual signal mask
/// and disposition modification, but the trait itself does not require them as
/// supertraits.
pub trait RunUnblocking: Signals {
    /// Runs the given function with the specified signal unblocked and the
    /// default disposition.
    ///
    /// This function restores the default disposition for the given signal,
    /// unblocks it, runs the given function, and then restores the original
    /// signal mask and disposition. If all operations succeed, the result of
    /// the function is returned. If any operation fails, an error is returned.
    ///
    /// This function restores the original signal mask and disposition even if
    /// the given function returns an error, in which case any error restoring
    /// the signal mask or disposition is discarded. If the signal cannot be
    /// unblocked or the disposition cannot be changed, this function returns an
    /// error without running the function.
    fn run_unblocking<F, T>(
        &self,
        signal: signal::Number,
        f: F,
    ) -> impl Future<Output = Result<T>> + use<'_, Self, F, T>
    where
        F: AsyncFnOnce() -> Result<T>;
}

impl<S> RunUnblocking for S
where
    S: Sigmask + Sigaction + ?Sized,
{
    async fn run_unblocking<F, T>(&self, signal: signal::Number, f: F) -> Result<T>
    where
        F: AsyncFnOnce() -> Result<T>,
    {
        if signal == S::SIGKILL || signal == S::SIGSTOP {
            // These signals cannot be ignored or blocked, so we just run the function.
            return f().await;
        }

        let sigset = S::Sigset::from_signals([signal])?;

        let old_handling = self.sigaction(signal, Disposition::Default)?;

        let mut old_mask = S::Sigset::new();
        let unblock_result = self
            .sigmask(Some((SigmaskOp::Remove, &sigset)), Some(&mut old_mask))
            .await;
        if let Err(e) = unblock_result {
            _ = self.sigaction(signal, old_handling);
            return Err(e);
        }

        let main_result = f().await;

        let restore_mask_result = self.sigmask(Some((SigmaskOp::Set, &old_mask)), None).await;
        let restore_action_result = self.sigaction(signal, old_handling);

        if main_result.is_ok() {
            restore_mask_result?;
            restore_action_result?;
        }
        main_result
    }
}

/// Switches the foreground process group with the default SIGTTOU settings.
///
/// This is a convenience function to ensure the shell has been in the
/// foreground and optionally change the foreground process group. If you call
/// [`TcSetPgrp::tcsetpgrp`] from a background process that has not ignored or
/// blocked SIGTTOU, the process is stopped by SIGTTOU. This behavior can be
/// used to ensure the shell is in the foreground before starting job control
/// operations.
///
/// This function temporarily restores the default disposition for SIGTTOU and
/// unblocks it while calling `tcsetpgrp`, which ensures that the shell is
/// suspended if it is not in the foreground. The suspended shell must be
/// resumed by another job-controlling process, after which this function
/// continues. If the shell is already in the foreground, this function behaves
/// the same as usual `tcsetpgrp`.
///
/// To simply make sure the shell is in the foreground without changing the
/// foreground job, you can call this function with `pgid` set to the process
/// group ID of the current process.
///
/// Use [`tcsetpgrp_with_block`] to change the job even if the current shell is
/// not in the foreground.
pub async fn tcsetpgrp_without_block<S>(system: &S, fd: Fd, pgid: Pid) -> Result<()>
where
    S: RunUnblocking + TcSetPgrp + ?Sized,
{
    system
        .run_unblocking(S::SIGTTOU, || system.tcsetpgrp(fd, pgid))
        .await
}
