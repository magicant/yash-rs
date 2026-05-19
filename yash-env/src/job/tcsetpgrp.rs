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
use crate::system::{
    Disposition, Result, Sigaction, Sigmask, SigmaskOp, Signals, Sigset as _, TcSetPgrp,
};

/// Switches the foreground process group with SIGTTOU blocked.
///
/// This is a convenience function to change the foreground process group
/// safely. If you call [`TcSetPgrp::tcsetpgrp`] from a background process, the
/// process is stopped by SIGTTOU by default. To prevent this effect, SIGTTOU
/// must be blocked or ignored when `tcsetpgrp` is called. This function uses
/// [`Sigmask::sigmask`] to block SIGTTOU before calling `tcsetpgrp` and also to
/// restore the original signal mask after `tcsetpgrp`.
///
/// Use [`tcsetpgrp_without_block`] if you need to make sure the shell is in the
/// foreground before changing the foreground job.
pub async fn tcsetpgrp_with_block<S>(system: &S, fd: Fd, pgid: Pid) -> Result<()>
where
    S: Signals + Sigmask + TcSetPgrp + ?Sized,
{
    let mut old_mask = S::Sigset::new();
    system
        .sigmask(
            Some((SigmaskOp::Add, &S::Sigset::from_signals([S::SIGTTOU])?)),
            Some(&mut old_mask),
        )
        .await?;

    let result = system.tcsetpgrp(fd, pgid).await;

    let result_2 = system
        .sigmask(Some((SigmaskOp::Set, &old_mask)), None)
        .await;

    result.and(result_2)
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
