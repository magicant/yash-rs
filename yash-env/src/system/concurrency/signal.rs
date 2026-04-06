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

//! Implementation of `Concurrent` related to signals

use super::Concurrent;
use crate::signal::Number;
use crate::system::{Disposition, Errno, Sigaction, Sigmask, SigmaskOp};
use crate::trap::SignalSystem;
use std::rc::Rc;

impl<S> SignalSystem for Rc<Concurrent<S>>
where
    S: Sigmask + Sigaction,
{
    fn get_disposition(&self, signal: Number) -> Result<Disposition, Errno> {
        self.inner.get_sigaction(signal)
    }

    fn set_disposition(
        &self,
        signal: Number,
        disposition: Disposition,
    ) -> impl Future<Output = Result<Disposition, Errno>> + use<S> {
        let this = Rc::clone(self);
        async move {
            // TODO When changing the disposition to Catch, we should first block the signal, then change the disposition.
            let old_action = this.inner.sigaction(signal, disposition);
            let op = match disposition {
                Disposition::Default => SigmaskOp::Remove,
                // TODO For Ignore, we should actually unblock
                Disposition::Ignore | Disposition::Catch => SigmaskOp::Add,
            };
            this.inner.sigmask(Some((op, &[signal])), None).await?;
            old_action
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{ProcessResult, ProcessState};
    use crate::system::SendSignal as _;
    use crate::system::r#virtual::{SIGQUIT, SIGTERM, VirtualSystem};
    use futures_util::FutureExt as _;

    #[test]
    fn setting_disposition_from_default_to_catch() {
        let inner = VirtualSystem::new();
        let system = Rc::new(Concurrent::new(inner.clone()));
        let result = system
            .set_disposition(SIGTERM, Disposition::Catch)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Disposition::Default));
        assert_eq!(system.get_disposition(SIGTERM), Ok(Disposition::Catch));

        // When the disposition is set to `Catch`, the signal is blocked.
        inner.raise(SIGTERM).now_or_never().unwrap().unwrap();
        // So the process should still be running.
        assert_eq!(inner.current_process().state(), ProcessState::Running);
    }

    #[test]
    fn setting_disposition_from_default_to_ignore() {
        let inner = VirtualSystem::new();
        let system = Rc::new(Concurrent::new(inner.clone()));
        let result = system
            .set_disposition(SIGTERM, Disposition::Ignore)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Disposition::Default));
        assert_eq!(system.get_disposition(SIGTERM), Ok(Disposition::Ignore));

        // Since the signal is ignored, sending it should have no effect.
        inner.raise(SIGTERM).now_or_never().unwrap().unwrap();
        assert_eq!(inner.current_process().state(), ProcessState::Running);
    }

    #[test]
    fn setting_disposition_from_ignore_to_catch() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        system
            .set_disposition(SIGQUIT, Disposition::Ignore)
            .now_or_never()
            .unwrap()
            .unwrap();

        let result = system
            .set_disposition(SIGQUIT, Disposition::Catch)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(Disposition::Ignore));
        assert_eq!(system.get_disposition(SIGQUIT), Ok(Disposition::Catch));
    }

    #[test]
    fn setting_disposition_from_catch_to_default() {
        let inner = VirtualSystem::new();
        let system = Rc::new(Concurrent::new(inner.clone()));
        system
            .set_disposition(SIGQUIT, Disposition::Catch)
            .now_or_never()
            .unwrap()
            .unwrap();
        // When the disposition is set to `Catch`, the signal is blocked.
        system.raise(SIGQUIT).now_or_never().unwrap().unwrap();

        // Resetting the disposition to `Default` should unblock the signal,
        // which should cause the process to be terminated.
        let result = system
            .set_disposition(SIGQUIT, Disposition::Default)
            .now_or_never();
        assert_eq!(result, None);
        assert_eq!(
            inner.current_process().state(),
            ProcessState::Halted(ProcessResult::Signaled {
                signal: SIGQUIT,
                core_dump: true
            })
        );
    }
}
