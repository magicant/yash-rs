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

/// Switches the foreground process group with the default SIGTTOU settings.
///
/// This is a convenience function to ensure the shell has been in the
/// foreground and optionally change the foreground process group. This
/// function calls [`Sigaction::sigaction`] to restore the action for
/// SIGTTOU to the default disposition (which is to suspend the shell
/// process), [`Sigmask::sigmask`] to unblock SIGTTOU, and
/// [`TcSetPgrp::tcsetpgrp`] to modify the foreground job. If the calling
/// process is not in the foreground, `tcsetpgrp` will suspend the process
/// with SIGTTOU until another job-controlling process resumes it in the
/// foreground. After `tcsetpgrp` completes, this function calls `sigmask`
/// and `sigaction` to restore the original state.
///
/// Note that if `pgid` is the process group ID of the current process, this
/// function does not change the foreground job, but the process is still
/// subject to suspension if it has not been in the foreground.
///
/// Use [`tcsetpgrp_with_block`] to change the job even if the current shell is
/// not in the foreground.
pub async fn tcsetpgrp_without_block<S>(system: &S, fd: Fd, pgid: Pid) -> Result<()>
where
    S: Signals + Sigaction + Sigmask + TcSetPgrp + ?Sized,
{
    let sigttou = S::Sigset::from_signals([S::SIGTTOU])?;

    match system.sigaction(S::SIGTTOU, Disposition::Default) {
        Err(e) => Err(e),
        Ok(old_handling) => {
            let mut old_mask = S::Sigset::new();
            let result = match system
                .sigmask(Some((SigmaskOp::Remove, &sigttou)), Some(&mut old_mask))
                .await
            {
                Err(e) => Err(e),
                Ok(()) => {
                    let result = system.tcsetpgrp(fd, pgid).await;

                    let result_2 = system
                        .sigmask(Some((SigmaskOp::Set, &old_mask)), None)
                        .await;

                    result.and(result_2)
                }
            };

            let result_2 = system.sigaction(S::SIGTTOU, old_handling).map(drop);

            result.and(result_2)
        }
    }
}
