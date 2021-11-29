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

//! Type definitions for signal handling settings.

use crate::system::{Errno, SignalHandling};
#[cfg(doc)]
use crate::system::{SharedSystem, System};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use yash_syntax::source::Location;

#[doc(no_inline)]
pub use nix::sys::signal::Signal;

/// System interface for signal handling configuration.
pub trait SignalSystem {
    /// Sets how a signal is handled.
    ///
    /// This function updates the signal blocking mask and the signal action for
    /// the specified signal and remembers the previous configuration for
    /// restoration.
    ///
    /// Returns the previous handler.
    fn set_signal_handling(
        &mut self,
        signal: Signal,
        handling: SignalHandling,
    ) -> Result<SignalHandling, Errno>;
}

/// Action performed when a signal is delivered to the shell process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Trap {
    /// Performs the default signal action.
    ///
    /// The behavior depends on the signal delivered.
    Default,

    /// Ignores the delivered signal.
    Ignore,

    /// Executes a command string.
    Command(String),
}

impl Default for Trap {
    fn default() -> Self {
        Trap::Default
    }
}

impl From<&Trap> for SignalHandling {
    fn from(trap: &Trap) -> Self {
        match trap {
            Trap::Default => SignalHandling::Default,
            Trap::Ignore => SignalHandling::Ignore,
            Trap::Command(_) => SignalHandling::Catch,
        }
    }
}

/// Error that may happen in [`TrapSet::set_trap`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetTrapError {
    /// Attempt to set a trap that has been ignored since the shell startup.
    InitiallyIgnored,
    /// Attempt to set a trap for the `SIGKILL` signal.
    SIGKILL,
    /// Attempt to set a trap for the `SIGSTOP` signal.
    SIGSTOP,
    /// Error from the underlying system interface.
    SystemError(Errno),
}

impl From<Errno> for SetTrapError {
    fn from(errno: Errno) -> Self {
        SetTrapError::SystemError(errno)
    }
}

/// State of the trap action for a signal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrapState {
    /// Action taken when the signal is delivered to the shell process.
    pub action: Trap,
    /// Location of the simple command that invoked the trap built-in that has set this trap.
    pub origin: Location,
    /// True iff the signal has been caught and the trap command has not yet executed.
    pub pending: bool,
}

/// User-visible signal disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
enum UserSignalState {
    /// The user has not yet set a trap for the signal, and the disposition the
    /// shell has inherited from the pre-exec process is `SIG_DFL`.
    InitiallyDefaulted,
    /// The user has not yet set a trap for the signal, and the disposition the
    /// shell has inherited from the pre-exec process is `SIG_IGN`.
    InitiallyIgnored,
    /// User-defined trap.
    Trap(TrapState),
}

impl From<&UserSignalState> for SignalHandling {
    fn from(state: &UserSignalState) -> Self {
        match state {
            UserSignalState::InitiallyDefaulted => SignalHandling::Default,
            UserSignalState::InitiallyIgnored => SignalHandling::Ignore,
            UserSignalState::Trap(trap) => (&trap.action).into(),
        }
    }
}

#[derive(Clone, Debug)]
struct SignalState {
    current_user_state: UserSignalState,
    internal_handler_enabled: bool,
}

/// Iterator of trap actions configured in a [trap set](TrapSet).
#[must_use]
pub struct Iter<'a> {
    inner: std::collections::btree_map::Iter<'a, Signal, SignalState>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a Signal, &'a TrapState);
    fn next(&mut self) -> Option<(&'a Signal, &'a TrapState)> {
        loop {
            let item = self.inner.next()?;
            if let UserSignalState::Trap(trap) = &item.1.current_user_state {
                return Some((item.0, trap));
            }
        }
    }
}

/// Collection of signal handling settings.
///
/// A `TrapSet` remembers the trap configured for each signal and manages the
/// signal handlers installed to the underlying system. `TrapSet` acts as a
/// decorator for a system implementing [`SignalSystem`]. Methods of `TrapSet`
/// expect to be passed the same system instance in every call.
///
/// `TrapSet` manages two types of signal handling configurations. One is
/// user-defined traps, which are explicitly configured by the trap built-in.
/// The other is internal handlers, which are installed by the internals of the
/// shell to implement additional actions the shell is required to perform.
///
/// `TrapSet` merges the two configurations into a single
/// [`system::SignalHandling`](SignalHandling) for each signal and sets it to
/// the system.
#[derive(Clone, Debug, Default)]
pub struct TrapSet {
    signals: BTreeMap<Signal, SignalState>,
}

// TODO Extend internal handlers for other signals
impl TrapSet {
    /// Returns the currently configured trap action for a signal.
    ///
    /// This function returns an optional pair of a trap action and the location
    /// specified when setting the trap. The result is `None` if no trap has
    /// been set for the signal.
    ///
    /// This function does not reflect the initial signal actions the shell
    /// inherited on startup.
    pub fn get_trap(&self, signal: Signal) -> Option<&TrapState> {
        self.signals.get(&signal).and_then(|state| {
            if let UserSignalState::Trap(trap) = &state.current_user_state {
                Some(trap)
            } else {
                None
            }
        })
    }

    /// Sets a trap action for a signal.
    ///
    /// This function installs a signal handler to the specified underlying
    /// system.
    ///
    /// If `override_ignore` is `false`, you cannot set a trap for a signal that
    /// has been ignored since the shell startup. An interactive shell should
    /// set `override_ignore` to `true` to bypass this restriction.
    ///
    /// You can never set a trap for `SIGKILL` or `SIGSTOP`.
    ///
    /// `origin` should be the location of the command performing this trap
    /// update. It is only informative: It does not affect the signal handling
    /// behavior and can be referenced later by [`get_trap`](Self::get_trap).
    pub fn set_trap<S: SignalSystem>(
        &mut self,
        system: &mut S,
        signal: Signal,
        action: Trap,
        origin: Location,
        override_ignore: bool,
    ) -> Result<(), SetTrapError> {
        match signal {
            Signal::SIGKILL => return Err(SetTrapError::SIGKILL),
            Signal::SIGSTOP => return Err(SetTrapError::SIGSTOP),
            _ => (),
        }

        let state = TrapState {
            action,
            origin,
            pending: false,
        };

        let entry = match self.signals.entry(signal) {
            Entry::Vacant(vacant) => {
                if !override_ignore {
                    let initial_handling =
                        system.set_signal_handling(signal, SignalHandling::Ignore)?;
                    if initial_handling == SignalHandling::Ignore {
                        vacant.insert(SignalState {
                            current_user_state: UserSignalState::InitiallyIgnored,
                            internal_handler_enabled: false,
                        });
                        return Err(SetTrapError::InitiallyIgnored);
                    }
                }
                Entry::Vacant(vacant)
            }
            Entry::Occupied(mut occupied) => {
                if !override_ignore
                    && occupied.get().current_user_state == UserSignalState::InitiallyIgnored
                {
                    return Err(SetTrapError::InitiallyIgnored);
                }
                if occupied.get().internal_handler_enabled {
                    occupied.get_mut().current_user_state = UserSignalState::Trap(state);
                    return Ok(());
                }
                Entry::Occupied(occupied)
            }
        };

        system.set_signal_handling(signal, (&state.action).into())?;

        let state = SignalState {
            current_user_state: UserSignalState::Trap(state),
            internal_handler_enabled: false,
        };
        #[allow(clippy::drop_ref)]
        match entry {
            Entry::Vacant(vacant) => drop(vacant.insert(state)),
            Entry::Occupied(mut occupied) => drop(occupied.insert(state)),
        }

        Ok(())
    }

    /// Returns an iterator over the currently configured signal actions.
    pub fn iter(&self) -> Iter {
        let inner = self.signals.iter();
        Iter { inner }
    }

    /// Resets existing `Trap::Command` settings to the default.
    ///
    /// POSIX requires that traps other than `Trap::Ignore` be reset when
    /// entering a subshell. This function achieves that effect.
    pub fn enter_subshell<S: SignalSystem>(&mut self, system: &mut S) {
        for (&signal, state) in &mut self.signals {
            if let UserSignalState::Trap(trap) = &state.current_user_state {
                if let Trap::Command(_) = &trap.action {
                    state.current_user_state = UserSignalState::InitiallyDefaulted;
                    if !state.internal_handler_enabled {
                        system
                            .set_signal_handling(signal, crate::system::SignalHandling::Default)
                            .ok();
                    }
                }
            }
        }
    }

    /// Sets the `pending` flag of the [`TrapState`] for the specified signal.
    ///
    /// This function does nothing if no trap action has been
    /// [set](Self::set_trap) for the signal.
    pub fn catch_signal(&mut self, signal: Signal) {
        if let Some(state) = self.signals.get_mut(&signal) {
            if let UserSignalState::Trap(trap) = &mut state.current_user_state {
                trap.pending = true;
            }
        }
    }

    /// Returns a signal that has been [caught](Self::catch_signal).
    ///
    /// This function clears the `pending` flag of the [`TrapState`] for the
    /// specified signal.
    ///
    /// If there is more than one caught signal, it is unspecified which one of
    /// them is returned. If there is no caught signal, `None` is returned.
    pub fn take_caught_signal(&mut self) -> Option<(Signal, &TrapState)> {
        self.signals
            .iter_mut()
            .find_map(|(signal, state)| match &mut state.current_user_state {
                UserSignalState::Trap(trap) if trap.pending => {
                    trap.pending = false;
                    Some((*signal, &*trap))
                }
                _ => None,
            })
    }

    /// Installs an internal handler for `SIGCHLD`.
    ///
    /// You should install the `SIGCHLD` handler to the system by using this
    /// function before waiting for `SIGCHLD` with [`System::wait`] and
    /// [`SharedSystem::wait_for_signal`].
    ///
    /// This function remembers that the handler has been installed, so a second
    /// call to the function will be a no-op.
    pub fn enable_sigchld_handler<S: SignalSystem>(&mut self, system: &mut S) -> Result<(), Errno> {
        let entry = self.signals.entry(Signal::SIGCHLD);
        if let Entry::Occupied(occupied) = &entry {
            if occupied.get().internal_handler_enabled {
                return Ok(());
            }
        }

        let previous_handler =
            system.set_signal_handling(Signal::SIGCHLD, SignalHandling::Catch)?;

        match entry {
            Entry::Occupied(mut occupied) => {
                occupied.get_mut().internal_handler_enabled = true;
            }
            Entry::Vacant(vacant) => {
                let current_user_state = if previous_handler == SignalHandling::Ignore {
                    UserSignalState::InitiallyIgnored
                } else {
                    UserSignalState::InitiallyDefaulted
                };
                vacant.insert(SignalState {
                    current_user_state,
                    internal_handler_enabled: true,
                });
            }
        }

        Ok(())
    }

    /// Uninstalls all internal handlers.
    ///
    /// This function removes all internal handlers that have been previously
    /// installed by `self`. It leaves handlers for any existing user-defined
    /// traps.
    pub fn disable_internal_handlers<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        if let Some(state) = self.signals.get_mut(&Signal::SIGCHLD) {
            if state.internal_handler_enabled {
                system.set_signal_handling(Signal::SIGCHLD, (&state.current_user_state).into())?;
                state.internal_handler_enabled = false;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct DummySystem(HashMap<Signal, SignalHandling>);

    impl SignalSystem for DummySystem {
        fn set_signal_handling(
            &mut self,
            signal: Signal,
            handling: SignalHandling,
        ) -> Result<SignalHandling, Errno> {
            Ok(self
                .0
                .insert(signal, handling)
                .unwrap_or(SignalHandling::Default))
        }
    }

    #[test]
    fn default_trap() {
        let trap_set = TrapSet::default();
        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), None);
    }

    #[test]
    fn setting_trap_to_ignore() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");

        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGCHLD,
            Trap::Ignore,
            origin.clone(),
            false,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action: Trap::Ignore,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }

    #[test]
    fn setting_trap_to_command() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Trap::Command("echo".to_string());
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGCHLD,
            action.clone(),
            origin.clone(),
            false,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Catch
        );
    }

    #[test]
    fn setting_trap_to_default() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("foo");
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, origin, false)
            .unwrap();

        let origin = Location::dummy("bar");
        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGCHLD,
            Trap::Default,
            origin.clone(),
            false,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action: Trap::Default,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Default
        );
    }

    #[test]
    fn resetting_trap_from_ignore_no_override() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("foo");
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, origin, false);
        assert_eq!(result, Err(SetTrapError::InitiallyIgnored));

        // Idempotence
        let origin = Location::dummy("bar");
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, origin, false);
        assert_eq!(result, Err(SetTrapError::InitiallyIgnored));

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), None);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }

    #[test]
    fn resetting_trap_from_ignore_override() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGCHLD,
            Trap::Ignore,
            origin.clone(),
            true,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action: Trap::Ignore,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }

    #[test]
    fn setting_trap_for_two_signals() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGUSR1,
            Trap::Ignore,
            origin_1.clone(),
            false,
        );
        assert_eq!(result, Ok(()));

        let command = Trap::Command("echo".to_string());
        let origin_2 = Location::dummy("bar");
        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGUSR2,
            command.clone(),
            origin_2.clone(),
            false,
        );
        assert_eq!(result, Ok(()));

        assert_eq!(
            trap_set.get_trap(Signal::SIGUSR1),
            Some(&TrapState {
                action: Trap::Ignore,
                origin: origin_1,
                pending: false
            })
        );
        assert_eq!(
            trap_set.get_trap(Signal::SIGUSR2),
            Some(&TrapState {
                action: command,
                origin: origin_2,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGUSR1],
            crate::system::SignalHandling::Ignore
        );
        assert_eq!(
            system.0[&Signal::SIGUSR2],
            crate::system::SignalHandling::Catch
        );
    }

    #[test]
    fn setting_trap_for_sigkill() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(&mut system, Signal::SIGKILL, Trap::Ignore, origin, false);
        assert_eq!(result, Err(SetTrapError::SIGKILL));
        assert_eq!(trap_set.get_trap(Signal::SIGKILL), None);
        assert_eq!(system.0.get(&Signal::SIGKILL), None);
    }

    #[test]
    fn setting_trap_for_sigstop() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(&mut system, Signal::SIGSTOP, Trap::Ignore, origin, false);
        assert_eq!(result, Err(SetTrapError::SIGSTOP));
        assert_eq!(trap_set.get_trap(Signal::SIGSTOP), None);
        assert_eq!(system.0.get(&Signal::SIGSTOP), None);
    }

    #[test]
    fn iteration() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        trap_set
            .set_trap(
                &mut system,
                Signal::SIGUSR1,
                Trap::Ignore,
                origin_1.clone(),
                false,
            )
            .unwrap();
        let command = Trap::Command("echo".to_string());
        let origin_2 = Location::dummy("bar");
        trap_set
            .set_trap(
                &mut system,
                Signal::SIGUSR2,
                command.clone(),
                origin_2.clone(),
                false,
            )
            .unwrap();

        let mut i = trap_set.iter();
        let first = i.next().unwrap();
        assert_eq!(first.0, &Signal::SIGUSR1);
        assert_eq!(first.1.action, Trap::Ignore);
        assert_eq!(first.1.origin, origin_1);
        let second = i.next().unwrap();
        assert_eq!(second.0, &Signal::SIGUSR2);
        assert_eq!(second.1.action, command);
        assert_eq!(second.1.origin, origin_2);
        assert_eq!(i.next(), None);
    }

    #[test]
    fn entering_subshell_resets_command_traps() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Trap::Command(String::new());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, action, origin, false)
            .unwrap();

        trap_set.enter_subshell(&mut system);
        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), None);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Default
        );
    }

    #[test]
    fn entering_subshell_keeps_ignore_traps() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(
                &mut system,
                Signal::SIGCHLD,
                Trap::Ignore,
                origin.clone(),
                false,
            )
            .unwrap();

        trap_set.enter_subshell(&mut system);
        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action: Trap::Ignore,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }

    #[test]
    fn entering_subshell_with_internal_handler() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Trap::Command(String::new());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, action, origin, false)
            .unwrap();
        trap_set.enable_sigchld_handler(&mut system).unwrap();

        trap_set.enter_subshell(&mut system);
        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), None);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Catch
        );
    }

    #[test]
    fn catching_signal() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let command = Trap::Command("echo INT".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGINT, command, origin, false)
            .unwrap();
        let command = Trap::Command("echo TERM".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGTERM, command, origin, false)
            .unwrap();

        trap_set.catch_signal(Signal::SIGCHLD);
        trap_set.catch_signal(Signal::SIGINT);

        let trap_state = trap_set.get_trap(Signal::SIGINT).unwrap();
        assert!(trap_state.pending, "{:?}", trap_state);
        let trap_state = trap_set.get_trap(Signal::SIGTERM).unwrap();
        assert!(!trap_state.pending, "{:?}", trap_state);
    }

    #[test]
    fn taking_caught_signal() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        assert_eq!(trap_set.take_caught_signal(), None);

        let command = Trap::Command("echo INT".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGINT, command, origin, false)
            .unwrap();
        let command = Trap::Command("echo TERM".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGTERM, command, origin, false)
            .unwrap();
        let command = Trap::Command("echo USR1".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(&mut system, Signal::SIGUSR1, command, origin, false)
            .unwrap();
        assert_eq!(trap_set.take_caught_signal(), None);

        trap_set.catch_signal(Signal::SIGINT);
        trap_set.catch_signal(Signal::SIGUSR1);
        // The order in which take_caught_signal returns the two signals is
        // unspecified, so we accept both the orders.
        let result = trap_set.take_caught_signal().unwrap();
        match result.0 {
            Signal::SIGINT => {
                assert_eq!(result.1.action, Trap::Command("echo INT".into()));
                assert!(!result.1.pending);

                let result = trap_set.take_caught_signal().unwrap();
                assert_eq!(result.0, Signal::SIGUSR1);
                assert_eq!(result.1.action, Trap::Command("echo USR1".into()));
                assert!(!result.1.pending);
            }
            Signal::SIGUSR1 => {
                assert_eq!(result.1.action, Trap::Command("echo USR1".into()));
                assert!(!result.1.pending);

                let result = trap_set.take_caught_signal().unwrap();
                assert_eq!(result.0, Signal::SIGINT);
                assert_eq!(result.1.action, Trap::Command("echo INT".into()));
                assert!(!result.1.pending);
            }
            _ => panic!("wrong signal: {:?}", result),
        }

        assert_eq!(trap_set.take_caught_signal(), None);
    }

    #[test]
    fn enabling_sigchld_handler() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Catch);
    }

    #[test]
    fn disabling_internal_handler_for_initially_defaulted_sigchld() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set.disable_internal_handlers(&mut system).unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Default);
    }

    #[test]
    fn disabling_internal_handler_for_initially_ignored_sigchld() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set.disable_internal_handlers(&mut system).unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Ignore);
    }

    #[test]
    fn disabling_internal_handler_after_enabling_twice() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set.disable_internal_handlers(&mut system).unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Ignore);
    }

    #[test]
    fn disabling_internal_handler_without_enabling() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.disable_internal_handlers(&mut system).unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Ignore);
    }

    #[test]
    fn reenabling_internal_handler() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set.disable_internal_handlers(&mut system).unwrap();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Catch);
    }

    #[test]
    fn setting_trap_to_ignore_after_enabling_internal_handler() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, origin, false);
        assert_eq!(result, Ok(()));
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Catch);
    }

    #[test]
    fn resetting_trap_from_ignore_no_override_after_enabling_internal_handler() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, origin, false);
        assert_eq!(result, Err(SetTrapError::InitiallyIgnored));
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Catch);
    }

    #[test]
    fn resetting_trap_from_ignore_override_after_enabling_internal_handler() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        let origin = Location::dummy("origin");
        let result = trap_set.set_trap(
            &mut system,
            Signal::SIGCHLD,
            Trap::Ignore,
            origin.clone(),
            true,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action: Trap::Ignore,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Catch
        );
    }

    #[test]
    fn disabling_internal_handler_with_ignore_trap() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        let origin = Location::dummy("origin");
        trap_set
            .set_trap(
                &mut system,
                Signal::SIGCHLD,
                Trap::Ignore,
                origin.clone(),
                false,
            )
            .unwrap();
        trap_set.disable_internal_handlers(&mut system).unwrap();

        assert_eq!(
            trap_set.get_trap(Signal::SIGCHLD),
            Some(&TrapState {
                action: Trap::Ignore,
                origin,
                pending: false
            })
        );
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }
}
