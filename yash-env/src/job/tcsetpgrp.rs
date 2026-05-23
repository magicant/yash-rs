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
    /// For most signals, this function restores the default disposition for the
    /// given signal, unblocks it, runs the given function, and then restores
    /// the original signal mask and disposition. If all operations succeed, the
    /// result of the function is returned. If any operation fails, an error is
    /// returned.
    ///
    /// This function restores the original signal mask and disposition even if
    /// the given function returns an error, in which case any error restoring
    /// the signal mask or disposition is discarded. If the signal cannot be
    /// unblocked or the disposition cannot be changed, this function returns an
    /// error without running the function.
    ///
    /// For signals whose mask or disposition cannot be changed (i.e., [SIGKILL]
    /// and [SIGSTOP]), this function does not try to unblock the signal or
    /// restore its default disposition and instead simply runs the given
    /// function.
    ///
    /// [SIGKILL]: crate::system::Signals::SIGKILL
    /// [SIGSTOP]: crate::system::Signals::SIGSTOP
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal;
    use crate::system::r#virtual::{
        SIGABRT, SIGALRM, SIGBUS, SIGCHLD, SIGCONT, SIGFPE, SIGHUP, SIGILL, SIGINT, SIGIOT,
        SIGKILL, SIGPIPE, SIGPROF, SIGQUIT, SIGSEGV, SIGSTOP, SIGSYS, SIGTERM, SIGTRAP, SIGTSTP,
        SIGTTIN, SIGTTOU, SIGURG, SIGUSR1, SIGUSR2, SIGVTALRM, SIGWINCH, SIGXCPU, SIGXFSZ,
    };
    use crate::system::{Errno, GetSigaction};
    use futures_util::FutureExt as _;
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, BTreeSet};
    use std::ops::RangeInclusive;

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    struct TestSigset(BTreeSet<signal::Number>);

    impl crate::system::Sigset for TestSigset {
        fn full() -> Self {
            unimplemented!("not needed for tests")
        }

        fn insert(&mut self, signal: signal::Number) -> Result<()> {
            self.0.insert(signal);
            Ok(())
        }

        fn remove(&mut self, signal: signal::Number) -> Result<()> {
            self.0.remove(&signal);
            Ok(())
        }

        fn contains(&self, signal: signal::Number) -> Result<bool> {
            Ok(self.0.contains(&signal))
        }
    }

    #[derive(Default)]
    struct MockSystem {
        mask: RefCell<TestSigset>,
        dispositions: RefCell<BTreeMap<signal::Number, Disposition>>,
        sigmask_errors: RefCell<BTreeMap<usize, Errno>>,
        sigaction_errors: RefCell<BTreeMap<usize, Errno>>,
        sigmask_call_count: Cell<usize>,
        sigaction_call_count: Cell<usize>,
    }

    impl MockSystem {
        fn set_mask(&self, mask: TestSigset) {
            self.mask.replace(mask);
        }

        fn set_disposition(&self, signal: signal::Number, disposition: Disposition) {
            self.dispositions.borrow_mut().insert(signal, disposition);
        }

        fn disposition_of(&self, signal: signal::Number) -> Disposition {
            self.dispositions
                .borrow()
                .get(&signal)
                .copied()
                .unwrap_or_default()
        }

        fn is_blocked(&self, signal: signal::Number) -> bool {
            self.mask
                .borrow()
                .contains(signal)
                .expect("signals in tests are always valid")
        }

        fn set_sigmask_error_on_call(&self, call: usize, error: Errno) {
            self.sigmask_errors.borrow_mut().insert(call, error);
        }

        fn set_sigaction_error_on_call(&self, call: usize, error: Errno) {
            self.sigaction_errors.borrow_mut().insert(call, error);
        }
    }

    impl Signals for MockSystem {
        const SIGABRT: signal::Number = SIGABRT;
        const SIGALRM: signal::Number = SIGALRM;
        const SIGBUS: signal::Number = SIGBUS;
        const SIGCHLD: signal::Number = SIGCHLD;
        const SIGCLD: Option<signal::Number> = None;
        const SIGCONT: signal::Number = SIGCONT;
        const SIGEMT: Option<signal::Number> = None;
        const SIGFPE: signal::Number = SIGFPE;
        const SIGHUP: signal::Number = SIGHUP;
        const SIGILL: signal::Number = SIGILL;
        const SIGINFO: Option<signal::Number> = None;
        const SIGINT: signal::Number = SIGINT;
        const SIGIO: Option<signal::Number> = None;
        const SIGIOT: signal::Number = SIGIOT;
        const SIGKILL: signal::Number = SIGKILL;
        const SIGLOST: Option<signal::Number> = None;
        const SIGPIPE: signal::Number = SIGPIPE;
        const SIGPOLL: Option<signal::Number> = None;
        const SIGPROF: signal::Number = SIGPROF;
        const SIGPWR: Option<signal::Number> = None;
        const SIGQUIT: signal::Number = SIGQUIT;
        const SIGSEGV: signal::Number = SIGSEGV;
        const SIGSTKFLT: Option<signal::Number> = None;
        const SIGSTOP: signal::Number = SIGSTOP;
        const SIGSYS: signal::Number = SIGSYS;
        const SIGTERM: signal::Number = SIGTERM;
        const SIGTHR: Option<signal::Number> = None;
        const SIGTRAP: signal::Number = SIGTRAP;
        const SIGTSTP: signal::Number = SIGTSTP;
        const SIGTTIN: signal::Number = SIGTTIN;
        const SIGTTOU: signal::Number = SIGTTOU;
        const SIGURG: signal::Number = SIGURG;
        const SIGUSR1: signal::Number = SIGUSR1;
        const SIGUSR2: signal::Number = SIGUSR2;
        const SIGVTALRM: signal::Number = SIGVTALRM;
        const SIGWINCH: signal::Number = SIGWINCH;
        const SIGXCPU: signal::Number = SIGXCPU;
        const SIGXFSZ: signal::Number = SIGXFSZ;

        fn sigrt_range(&self) -> Option<RangeInclusive<signal::Number>> {
            None
        }
    }

    impl Sigmask for MockSystem {
        type Sigset = TestSigset;

        fn sigmask(
            &self,
            op: Option<(SigmaskOp, &Self::Sigset)>,
            old_mask: Option<&mut Self::Sigset>,
        ) -> impl Future<Output = Result<()>> + use<> {
            let call_count = self.sigmask_call_count.get() + 1;
            self.sigmask_call_count.set(call_count);

            if let Some(error) = self.sigmask_errors.borrow_mut().remove(&call_count) {
                return std::future::ready(Err(error));
            }

            let result = {
                let mut mask = self.mask.borrow_mut();
                if let Some(old_mask) = old_mask {
                    old_mask.clone_from(&mask);
                }

                if let Some((op, signals)) = op {
                    match op {
                        SigmaskOp::Add => {
                            for &signal in &signals.0 {
                                mask.insert(signal).unwrap();
                            }
                        }
                        SigmaskOp::Remove => {
                            for &signal in &signals.0 {
                                mask.remove(signal).unwrap();
                            }
                        }
                        SigmaskOp::Set => {
                            *mask = signals.clone();
                        }
                    }
                }

                Ok(())
            };

            std::future::ready(result)
        }
    }

    impl GetSigaction for MockSystem {
        fn get_sigaction(&self, signal: signal::Number) -> Result<Disposition> {
            Ok(self.disposition_of(signal))
        }
    }

    impl Sigaction for MockSystem {
        fn sigaction(&self, signal: signal::Number, action: Disposition) -> Result<Disposition> {
            let call_count = self.sigaction_call_count.get() + 1;
            self.sigaction_call_count.set(call_count);

            if let Some(error) = self.sigaction_errors.borrow_mut().remove(&call_count) {
                return Err(error);
            }

            Ok(self
                .dispositions
                .borrow_mut()
                .insert(signal, action)
                .unwrap_or_default())
        }
    }

    #[test]
    fn run_blocking_blocks_signal_and_restores_mask_on_success() {
        let system = MockSystem::default();
        let called = Cell::new(false);

        let result = system
            .run_blocking(MockSystem::SIGTTOU, async || {
                called.set(true);
                assert!(system.is_blocked(MockSystem::SIGTTOU));
                Ok(())
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Ok(()));
        assert!(called.get());
        assert!(!system.is_blocked(MockSystem::SIGTTOU));
    }

    #[test]
    fn run_blocking_does_not_run_function_when_initial_sigmask_fails() {
        let system = MockSystem::default();
        system.set_sigmask_error_on_call(1, Errno::EINVAL);

        let result = system
            .run_blocking(MockSystem::SIGTTOU, async || -> Result<()> {
                unreachable!("closure should not be called")
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EINVAL));
    }

    #[test]
    fn run_blocking_discards_restore_error_when_function_returns_error() {
        let system = MockSystem::default();
        system.set_sigmask_error_on_call(2, Errno::EPERM);

        let result = system
            .run_blocking(MockSystem::SIGTTOU, async || Err::<(), _>(Errno::EINTR))
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EINTR));
    }

    #[test]
    fn run_blocking_propagates_restore_error_when_function_succeeds() {
        let system = MockSystem::default();
        system.set_sigmask_error_on_call(2, Errno::EPERM);

        let result = system
            .run_blocking(MockSystem::SIGTTOU, async || Ok(()))
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EPERM));
    }

    #[test]
    fn run_unblocking_for_sigkill_runs_function_without_sigaction_or_sigmask() {
        let system = MockSystem::default();
        let called = Cell::new(false);

        let result = system
            .run_unblocking(MockSystem::SIGKILL, async || {
                called.set(true);
                Err::<(), _>(Errno::EINTR)
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EINTR));
        assert!(called.get());
        assert_eq!(system.sigmask_call_count.get(), 0);
        assert_eq!(system.sigaction_call_count.get(), 0);
    }

    #[test]
    fn run_unblocking_for_sigstop_runs_function_without_sigaction_or_sigmask() {
        let system = MockSystem::default();
        let called = Cell::new(false);

        let result = system
            .run_unblocking(MockSystem::SIGSTOP, async || {
                called.set(true);
                Err::<(), _>(Errno::EINTR)
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EINTR));
        assert!(called.get());
        assert_eq!(system.sigmask_call_count.get(), 0);
        assert_eq!(system.sigaction_call_count.get(), 0);
    }

    #[test]
    fn run_unblocking_sets_default_unblocks_and_restores_on_success() {
        let system = MockSystem::default();
        let called = Cell::new(false);
        let mut initial_mask = TestSigset::new();
        initial_mask.insert(MockSystem::SIGTTOU).unwrap();
        system.set_mask(initial_mask.clone());
        system.set_disposition(MockSystem::SIGTTOU, Disposition::Ignore);

        let result = system
            .run_unblocking(MockSystem::SIGTTOU, async || {
                called.set(true);
                assert_eq!(
                    system.disposition_of(MockSystem::SIGTTOU),
                    Disposition::Default
                );
                assert!(!system.is_blocked(MockSystem::SIGTTOU));
                Ok(())
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Ok(()));
        assert!(called.get());
        assert!(system.is_blocked(MockSystem::SIGTTOU));
        assert_eq!(
            system.disposition_of(MockSystem::SIGTTOU),
            Disposition::Ignore
        );
    }

    #[test]
    fn run_unblocking_returns_error_when_unblock_sigmask_fails_and_restores_disposition() {
        let system = MockSystem::default();
        system.set_disposition(MockSystem::SIGTTOU, Disposition::Ignore);
        system.set_sigmask_error_on_call(1, Errno::EPERM);

        let result = system
            .run_unblocking(MockSystem::SIGTTOU, async || -> Result<()> {
                unreachable!("closure should not be called")
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EPERM));
        assert_eq!(
            system.disposition_of(MockSystem::SIGTTOU),
            Disposition::Ignore
        );
    }

    #[test]
    fn run_unblocking_discards_restore_errors_when_function_returns_error() {
        let system = MockSystem::default();
        let called = Cell::new(false);
        system.set_sigmask_error_on_call(2, Errno::EPERM);
        system.set_sigaction_error_on_call(2, Errno::EINVAL);

        let result = system
            .run_unblocking(MockSystem::SIGTTOU, async || {
                called.set(true);
                Err::<(), _>(Errno::EINTR)
            })
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EINTR));
        assert!(called.get());
    }

    #[test]
    fn run_unblocking_propagates_restore_mask_error_when_function_succeeds() {
        let system = MockSystem::default();
        system.set_sigmask_error_on_call(2, Errno::EPERM);

        let result = system
            .run_unblocking(MockSystem::SIGTTOU, async || Ok(()))
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EPERM));
    }

    #[test]
    fn run_unblocking_restores_sigaction_on_sigmask_restoration_error() {
        let system = MockSystem::default();
        system.set_disposition(MockSystem::SIGTTOU, Disposition::Ignore);
        system.set_sigmask_error_on_call(2, Errno::EPERM);

        let result = system
            .run_unblocking(MockSystem::SIGTTOU, async || Ok(()))
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EPERM));
        assert_eq!(
            system.disposition_of(MockSystem::SIGTTOU),
            Disposition::Ignore
        );
    }

    #[test]
    fn run_unblocking_propagates_restore_action_error_when_function_succeeds() {
        let system = MockSystem::default();
        system.set_sigaction_error_on_call(2, Errno::EINVAL);

        let result = system
            .run_unblocking(MockSystem::SIGTTOU, async || Ok(()))
            .now_or_never()
            .unwrap();

        assert_eq!(result, Err(Errno::EINVAL));
    }
}
