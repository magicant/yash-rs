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

/// Implementation of `SignalSystem` for `Concurrent`
///
/// `Concurrent` controls both the signal dispositions and the signal mask, so
/// it can receive and handle signals without race conditions.
impl<S> SignalSystem for Rc<Concurrent<S>>
where
    S: Sigmask + Sigaction,
{
    /// Returns the current disposition of the specified signal.
    ///
    /// This implementation simply forwards the call to the inner system's
    /// [`GetSigaction::get_sigaction`](crate::system::GetSigaction::get_sigaction)
    /// method.
    fn get_disposition(&self, signal: Number) -> Result<Disposition, Errno> {
        self.inner.get_sigaction(signal)
    }

    /// Sets the disposition of the specified signal to the given value and
    /// returns the old disposition.
    ///
    /// This implementation both updates the signal disposition and the signal
    /// mask to ensure that the [`select`](Concurrent::select) method can
    /// respond to received signals without race conditions.
    fn set_disposition(
        &self,
        signal: Number,
        disposition: Disposition,
    ) -> impl Future<Output = Result<Disposition, Errno>> + use<S> {
        let this = Rc::clone(self);
        async move {
            if disposition == Disposition::Catch {
                // Before setting the disposition to `Catch`, we need to block the signal
                // to prevent it from being delivered before the disposition is updated.
                this.sigmask(SigmaskOp::Add, signal).await?;
            }

            let old_action = this.inner.sigaction(signal, disposition)?;

            if disposition != Disposition::Catch {
                // After setting the disposition to `Default` or `Ignore`, we need to unblock
                // the signal to allow it to be delivered if it was previously blocked.
                this.sigmask(SigmaskOp::Remove, signal).await?;
            }

            Ok(old_action)
        }
    }
}

impl<S> Concurrent<S>
where
    S: Sigmask,
{
    /// Wrapper of the inner system's [`Sigmask::sigmask`] method that also
    /// updates the `select_mask` field.
    async fn sigmask(&self, op: SigmaskOp, signal: Number) -> Result<(), Errno> {
        let mut old_mask = Vec::new();
        self.inner
            .sigmask(Some((op, &[signal])), Some(&mut old_mask))
            .await?;

        self.state
            .borrow_mut()
            .select_mask
            .get_or_insert(old_mask)
            .retain(|&s| s != signal);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{ProcessResult, ProcessState};
    use crate::system::SendSignal as _;
    use crate::system::r#virtual::{SIGQUIT, SIGTERM, SIGUSR1, VirtualSystem};
    use futures_util::FutureExt as _;
    use std::num::NonZero;

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

    #[test]
    fn first_sigmask_updates_blocking_mask() {
        let inner = VirtualSystem::new();
        _ = inner
            .current_process_mut()
            .block_signals(SigmaskOp::Set, &[SIGQUIT, SIGTERM, SIGUSR1]);
        let system = Rc::new(Concurrent::new(inner.clone()));

        let result = system
            .sigmask(SigmaskOp::Add, SIGTERM)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(()));
        let blocked_signals = inner
            .current_process()
            .blocked_signals()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(blocked_signals, [SIGQUIT, SIGTERM, SIGUSR1]);
    }

    #[test]
    fn first_sigmask_sets_select_mask() {
        let inner = VirtualSystem::new();
        _ = inner
            .current_process_mut()
            .block_signals(SigmaskOp::Set, &[SIGQUIT, SIGTERM, SIGUSR1]);
        let system = Rc::new(Concurrent::new(inner.clone()));

        system
            .sigmask(SigmaskOp::Add, SIGTERM)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            system.state.borrow().select_mask.as_deref(),
            Some([SIGQUIT, SIGUSR1].as_slice())
        );
    }

    #[ignore = "current VirtualSystem::sigmask silently ignores invalid signals"]
    #[test]
    fn first_sigmask_leaves_select_mask_unchanged_on_error() {
        let system = Rc::new(Concurrent::new(VirtualSystem::new()));
        let invalid_signal = Number::from_raw_unchecked(NonZero::new(-1).unwrap());
        let result = system
            .sigmask(SigmaskOp::Add, invalid_signal)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::EINVAL));
        assert_eq!(system.state.borrow().select_mask.as_deref(), None);
    }

    #[test]
    fn second_sigmask_updates_select_mask() {
        let inner = VirtualSystem::new();
        _ = inner
            .current_process_mut()
            .block_signals(SigmaskOp::Set, &[SIGQUIT, SIGTERM, SIGUSR1]);
        let system = Rc::new(Concurrent::new(inner.clone()));

        system
            .sigmask(SigmaskOp::Add, SIGTERM)
            .now_or_never()
            .unwrap()
            .unwrap();
        system
            .sigmask(SigmaskOp::Remove, SIGQUIT)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(
            system.state.borrow().select_mask.as_deref(),
            Some([SIGUSR1].as_slice())
        );
    }
}
