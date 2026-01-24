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

#[cfg(doc)]
use super::SharedSystem;
use super::{Pid, Result};
pub use crate::signal::{Name, Number, RawNumber};
use std::borrow::Cow;
use std::num::NonZero;
use std::ops::RangeInclusive;

/// Trait for managing available signals
pub trait Signals {
    /// The signal number for `SIGABRT`
    const SIGABRT: Number;
    /// The signal number for `SIGALRM`
    const SIGALRM: Number;
    /// The signal number for `SIGBUS`
    const SIGBUS: Number;
    /// The signal number for `SIGCHLD`
    const SIGCHLD: Number;
    /// The signal number for `SIGCLD`, if available on the system
    const SIGCLD: Option<Number>;
    /// The signal number for `SIGCONT`
    const SIGCONT: Number;
    /// The signal number for `SIGEMT`, if available on the system
    const SIGEMT: Option<Number>;
    /// The signal number for `SIGFPE`
    const SIGFPE: Number;
    /// The signal number for `SIGHUP`
    const SIGHUP: Number;
    /// The signal number for `SIGILL`
    const SIGILL: Number;
    /// The signal number for `SIGINFO`, if available on the system
    const SIGINFO: Option<Number>;
    /// The signal number for `SIGINT`
    const SIGINT: Number;
    /// The signal number for `SIGIO`, if available on the system
    const SIGIO: Option<Number>;
    /// The signal number for `SIGIOT`
    const SIGIOT: Number;
    /// The signal number for `SIGKILL`
    const SIGKILL: Number;
    /// The signal number for `SIGLOST`, if available on the system
    const SIGLOST: Option<Number>;
    /// The signal number for `SIGPIPE`
    const SIGPIPE: Number;
    /// The signal number for `SIGPOLL`, if available on the system
    const SIGPOLL: Option<Number>;
    /// The signal number for `SIGPROF`
    const SIGPROF: Number;
    /// The signal number for `SIGPWR`, if available on the system
    const SIGPWR: Option<Number>;
    /// The signal number for `SIGQUIT`
    const SIGQUIT: Number;
    /// The signal number for `SIGSEGV`
    const SIGSEGV: Number;
    /// The signal number for `SIGSTKFLT`, if available on the system
    const SIGSTKFLT: Option<Number>;
    /// The signal number for `SIGSTOP`
    const SIGSTOP: Number;
    /// The signal number for `SIGSYS`
    const SIGSYS: Number;
    /// The signal number for `SIGTERM`
    const SIGTERM: Number;
    /// The signal number for `SIGTHR`, if available on the system
    const SIGTHR: Option<Number>;
    /// The signal number for `SIGTRAP`
    const SIGTRAP: Number;
    /// The signal number for `SIGTSTP`
    const SIGTSTP: Number;
    /// The signal number for `SIGTTIN`
    const SIGTTIN: Number;
    /// The signal number for `SIGTTOU`
    const SIGTTOU: Number;
    /// The signal number for `SIGURG`
    const SIGURG: Number;
    /// The signal number for `SIGUSR1`
    const SIGUSR1: Number;
    /// The signal number for `SIGUSR2`
    const SIGUSR2: Number;
    /// The signal number for `SIGVTALRM`
    const SIGVTALRM: Number;
    /// The signal number for `SIGWINCH`
    const SIGWINCH: Number;
    /// The signal number for `SIGXCPU`
    const SIGXCPU: Number;
    /// The signal number for `SIGXFSZ`
    const SIGXFSZ: Number;

    /// Returns the range of real-time signals supported by the system.
    ///
    /// If the system does not support real-time signals, returns `None`.
    ///
    /// The range is provided as a method rather than associated constants
    /// because some systems determine the range at runtime.
    #[must_use]
    fn sigrt_range(&self) -> Option<RangeInclusive<Number>>;

    /// Returns an iterator over all real-time signals supported by the system.
    ///
    /// The iterator yields signal numbers in ascending order. If the system
    /// does not support real-time signals, the iterator yields no items.
    fn iter_sigrt(&self) -> impl DoubleEndedIterator<Item = Number> + use<Self> {
        let range = match self.sigrt_range() {
            Some(range) => range.start().as_raw()..=range.end().as_raw(),
            #[allow(clippy::reversed_empty_ranges)]
            None => 0..=-1,
        };
        // If NonZero implemented Step, we could use range.map(...)
        range.filter_map(|raw| NonZero::new(raw).map(Number::from_raw_unchecked))
    }

    /// Converts a signal number to its string representation.
    ///
    /// This function returns `Some(name)` if the signal number refers to a valid
    /// signal supported by the system. Otherwise, it returns `None`.
    ///
    /// The returned name does not include the `SIG` prefix.
    /// Note that one signal number can have multiple names, in which case it is
    /// unspecified which name is returned.
    #[must_use]
    fn sig2str<S: Into<RawNumber>>(&self, signal: S) -> Option<Cow<'static, str>> {
        fn inner<S: Signals + ?Sized>(
            system: &S,
            raw_number: RawNumber,
        ) -> Option<Cow<'static, str>> {
            let number = Number::from_raw_unchecked(NonZero::new(raw_number)?);
            // The signals below are ordered roughly by frequency of use
            // so that common names are preferred for signals with multiple names.
            if number == S::SIGABRT {
                Some(Cow::Borrowed("ABRT"))
            } else if number == S::SIGALRM {
                Some(Cow::Borrowed("ALRM"))
            } else if number == S::SIGBUS {
                Some(Cow::Borrowed("BUS"))
            } else if number == S::SIGCHLD {
                Some(Cow::Borrowed("CHLD"))
            } else if number == S::SIGCONT {
                Some(Cow::Borrowed("CONT"))
            } else if number == S::SIGFPE {
                Some(Cow::Borrowed("FPE"))
            } else if number == S::SIGHUP {
                Some(Cow::Borrowed("HUP"))
            } else if number == S::SIGILL {
                Some(Cow::Borrowed("ILL"))
            } else if number == S::SIGINT {
                Some(Cow::Borrowed("INT"))
            } else if number == S::SIGKILL {
                Some(Cow::Borrowed("KILL"))
            } else if number == S::SIGPIPE {
                Some(Cow::Borrowed("PIPE"))
            } else if number == S::SIGQUIT {
                Some(Cow::Borrowed("QUIT"))
            } else if number == S::SIGSEGV {
                Some(Cow::Borrowed("SEGV"))
            } else if number == S::SIGSTOP {
                Some(Cow::Borrowed("STOP"))
            } else if number == S::SIGTERM {
                Some(Cow::Borrowed("TERM"))
            } else if number == S::SIGTSTP {
                Some(Cow::Borrowed("TSTP"))
            } else if number == S::SIGTTIN {
                Some(Cow::Borrowed("TTIN"))
            } else if number == S::SIGTTOU {
                Some(Cow::Borrowed("TTOU"))
            } else if number == S::SIGUSR1 {
                Some(Cow::Borrowed("USR1"))
            } else if number == S::SIGUSR2 {
                Some(Cow::Borrowed("USR2"))
            } else if Some(number) == S::SIGPOLL {
                Some(Cow::Borrowed("POLL"))
            } else if number == S::SIGPROF {
                Some(Cow::Borrowed("PROF"))
            } else if number == S::SIGSYS {
                Some(Cow::Borrowed("SYS"))
            } else if number == S::SIGTRAP {
                Some(Cow::Borrowed("TRAP"))
            } else if number == S::SIGURG {
                Some(Cow::Borrowed("URG"))
            } else if number == S::SIGVTALRM {
                Some(Cow::Borrowed("VTALRM"))
            } else if number == S::SIGWINCH {
                Some(Cow::Borrowed("WINCH"))
            } else if number == S::SIGXCPU {
                Some(Cow::Borrowed("XCPU"))
            } else if number == S::SIGXFSZ {
                Some(Cow::Borrowed("XFSZ"))
            } else if Some(number) == S::SIGEMT {
                Some(Cow::Borrowed("EMT"))
            } else if Some(number) == S::SIGINFO {
                Some(Cow::Borrowed("INFO"))
            } else if Some(number) == S::SIGIO {
                Some(Cow::Borrowed("IO"))
            } else if Some(number) == S::SIGLOST {
                Some(Cow::Borrowed("LOST"))
            } else if Some(number) == S::SIGPWR {
                Some(Cow::Borrowed("PWR"))
            } else if Some(number) == S::SIGSTKFLT {
                Some(Cow::Borrowed("STKFLT"))
            } else if Some(number) == S::SIGTHR {
                Some(Cow::Borrowed("THR"))
            } else {
                let range = system.sigrt_range()?;
                if number == *range.start() {
                    Some(Cow::Borrowed("RTMIN"))
                } else if number == *range.end() {
                    Some(Cow::Borrowed("RTMAX"))
                } else if range.contains(&number) {
                    let rtmin = range.start().as_raw();
                    let rtmax = range.end().as_raw();
                    if raw_number <= rtmin.midpoint(rtmax) {
                        let offset = raw_number - rtmin;
                        Some(Cow::Owned(format!("RTMIN+{}", offset)))
                    } else {
                        let offset = rtmax - raw_number;
                        Some(Cow::Owned(format!("RTMAX-{}", offset)))
                    }
                } else {
                    None
                }
            }
        }
        inner(self, signal.into())
    }

    /// Converts a string representation of a signal to its signal number.
    ///
    /// This function returns `Some(number)` if the signal name is supported by
    /// the system. Otherwise, it returns `None`.
    ///
    /// The input name should not include the `SIG` prefix, and is case-sensitive.
    #[must_use]
    fn str2sig(&self, name: &str) -> Option<Number> {
        match name {
            "ABRT" => Some(Self::SIGABRT),
            "ALRM" => Some(Self::SIGALRM),
            "BUS" => Some(Self::SIGBUS),
            "CHLD" => Some(Self::SIGCHLD),
            "CLD" => Self::SIGCLD,
            "CONT" => Some(Self::SIGCONT),
            "EMT" => Self::SIGEMT,
            "FPE" => Some(Self::SIGFPE),
            "HUP" => Some(Self::SIGHUP),
            "ILL" => Some(Self::SIGILL),
            "INFO" => Self::SIGINFO,
            "INT" => Some(Self::SIGINT),
            "IO" => Self::SIGIO,
            "IOT" => Some(Self::SIGIOT),
            "KILL" => Some(Self::SIGKILL),
            "LOST" => Self::SIGLOST,
            "PIPE" => Some(Self::SIGPIPE),
            "POLL" => Self::SIGPOLL,
            "PROF" => Some(Self::SIGPROF),
            "PWR" => Self::SIGPWR,
            "QUIT" => Some(Self::SIGQUIT),
            "SEGV" => Some(Self::SIGSEGV),
            "STKFLT" => Self::SIGSTKFLT,
            "STOP" => Some(Self::SIGSTOP),
            "SYS" => Some(Self::SIGSYS),
            "TERM" => Some(Self::SIGTERM),
            "THR" => Self::SIGTHR,
            "TRAP" => Some(Self::SIGTRAP),
            "TSTP" => Some(Self::SIGTSTP),
            "TTIN" => Some(Self::SIGTTIN),
            "TTOU" => Some(Self::SIGTTOU),
            "URG" => Some(Self::SIGURG),
            "USR1" => Some(Self::SIGUSR1),
            "USR2" => Some(Self::SIGUSR2),
            "VTALRM" => Some(Self::SIGVTALRM),
            "WINCH" => Some(Self::SIGWINCH),
            "XCPU" => Some(Self::SIGXCPU),
            "XFSZ" => Some(Self::SIGXFSZ),
            _ => {
                enum BaseName {
                    Rtmin,
                    Rtmax,
                }
                let (basename, suffix) = if let Some(suffix) = name.strip_prefix("RTMIN") {
                    (BaseName::Rtmin, suffix)
                } else if let Some(suffix) = name.strip_prefix("RTMAX") {
                    (BaseName::Rtmax, suffix)
                } else {
                    return None;
                };
                if !suffix.is_empty() && !suffix.starts_with(['+', '-']) {
                    return None;
                }
                let range = self.sigrt_range()?;
                let base_raw = match basename {
                    BaseName::Rtmin => range.start().as_raw(),
                    BaseName::Rtmax => range.end().as_raw(),
                };
                let raw_number = if suffix.is_empty() {
                    base_raw
                } else {
                    let offset: RawNumber = suffix.parse().ok()?;
                    base_raw.checked_add(offset)?
                };
                let number = Number::from_raw_unchecked(NonZero::new(raw_number)?);
                range.contains(&number).then_some(number)
            }
        }
    }

    /// Tests if a signal number is valid.
    ///
    /// This function returns `Some((name, number))` if the signal number refers
    /// to a valid signal supported by the system. Otherwise, it returns `None`.
    ///
    /// Note that one signal number can have multiple names, in which case this
    /// function returns the name that is considered the most common.
    #[must_use]
    fn validate_signal(&self, number: RawNumber) -> Option<(Name, Number)>;

    /// Returns the signal name for the signal number.
    ///
    /// This function returns the signal name for the given signal number.
    ///
    /// If the signal number is invalid, this function panics. It may occur if
    /// the number is from a different system or was created without checking
    /// the validity.
    ///
    /// Note that one signal number can have multiple names, in which case this
    /// function returns the name that is considered the most common.
    #[must_use]
    fn signal_name_from_number(&self, number: Number) -> Name {
        self.validate_signal(number.as_raw()).unwrap().0
    }

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
        &self,
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

/// Trait for getting signal dispositions
pub trait GetSigaction: Signals {
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
    /// To change the disposition, use [`Sigaction::sigaction`].
    fn get_sigaction(&self, signal: Number) -> Result<Disposition>;
}

/// Trait for managing signal dispositions
pub trait Sigaction: GetSigaction {
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
    /// [`caught_signals`](CaughtSignals::caught_signals).
    ///
    /// To get the current disposition without changing it, use
    /// [`GetSigaction::get_sigaction`].
    fn sigaction(&self, signal: Number, action: Disposition) -> Result<Disposition>;
}

/// Trait for examining signals caught by the process
///
/// Implementors of this trait usually also implement [`Sigaction`] to allow
/// setting which signals are caught.
pub trait CaughtSignals {
    /// Returns signals this process has caught, if any.
    ///
    /// This is a low-level function used internally by
    /// [`SharedSystem::select`]. You should not call this function directly, or
    /// you will disrupt the behavior of `SharedSystem`. The description below
    /// applies if you want to do everything yourself without depending on
    /// `SharedSystem`.
    ///
    /// Implementors of this trait usually also implement [`Sigaction`] to allow
    /// setting which signals are caught.
    /// To catch a signal, you firstly install a signal handler by calling
    /// [`Sigaction::sigaction`] with [`Disposition::Catch`]. Once the handler
    /// is ready, signals sent to the process are accumulated in the
    /// implementor. Calling this function retrieves the list of caught signals.
    ///
    /// This function clears the internal list of caught signals, so a next call
    /// will return an empty list unless another signal is caught since the
    /// first call. Because the list size may be limited, you should call this
    /// function periodically before the list gets full, in which case further
    /// caught signals are silently ignored.
    ///
    /// Note that signals become pending if sent while blocked by
    /// [`Sigmask::sigmask`]. They must be unblocked so that they are caught and
    /// made available from this function.
    fn caught_signals(&self) -> Vec<Number>;
}

/// Trait for sending signals to processes
pub trait SendSignal {
    /// Sends a signal.
    ///
    /// This is a thin wrapper around the [`kill` system
    /// call](https://pubs.opengroup.org/onlinepubs/9799919799/functions/kill.html).
    /// If `signal` is `None`, permission to send a signal is checked, but no
    /// signal is sent.
    ///
    /// The virtual system version of this function blocks the calling thread if
    /// the signal stops or terminates the current process, hence returning a
    /// future. See [`VirtualSystem::kill`] for details.
    ///
    /// [`VirtualSystem::kill`]: crate::system::virtual::VirtualSystem::kill
    fn kill(
        &self,
        target: Pid,
        signal: Option<Number>,
    ) -> impl Future<Output = Result<()>> + use<Self>;

    /// Sends a signal to the current process.
    ///
    /// This is a thin wrapper around the `raise` system call.
    ///
    /// The virtual system version of this function blocks the calling thread if
    /// the signal stops or terminates the current process, hence returning a
    /// future. See [`VirtualSystem::kill`] for details.
    ///
    /// [`VirtualSystem::kill`]: crate::system::virtual::VirtualSystem::kill
    fn raise(&self, signal: Number) -> impl Future<Output = Result<()>> + use<Self>;
}
