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

#[derive(Clone, Debug, Eq, PartialEq)]
enum UserSignalState {
    InitiallyIgnored,
    Trap(Trap),
}

impl From<&UserSignalState> for SignalHandling {
    fn from(state: &UserSignalState) -> Self {
        match state {
            UserSignalState::InitiallyIgnored => SignalHandling::Ignore,
            UserSignalState::Trap(trap) => trap.into(),
        }
    }
}

#[derive(Clone, Debug)]
struct SignalState {
    user_state: UserSignalState,
    internal_handler_enabled: bool,
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
    /// This function does not reflect the initial signal actions the shell
    /// inherited on startup.
    pub fn get_trap(&self, signal: Signal) -> &Trap {
        if let Some(state) = self.signals.get(&signal) {
            if let UserSignalState::Trap(trap) = &state.user_state {
                return trap;
            }
        }

        const DEFAULT: &Trap = &Trap::Default;
        DEFAULT
    }

    /// Sets a trap action for a signal.
    ///
    /// This function installs a signal handler to the specified underlying
    /// system.
    ///
    /// If `override_ignore` is `false`, you cannot set the trap for a signal
    /// that has been ignored since the shell startup. An interactive shell
    /// should set `override_ignore` to `true` to bypass this restriction.
    ///
    /// You can never set the trap for `SIGKILL` and `SIGSTOP`.
    pub fn set_trap<S: SignalSystem>(
        &mut self,
        system: &mut S,
        signal: Signal,
        trap: Trap,
        override_ignore: bool,
    ) -> Result<(), SetTrapError> {
        match signal {
            Signal::SIGKILL => return Err(SetTrapError::SIGKILL),
            Signal::SIGSTOP => return Err(SetTrapError::SIGSTOP),
            _ => (),
        }

        let entry = match self.signals.entry(signal) {
            Entry::Vacant(vacant) => {
                if !override_ignore {
                    let initial_handling =
                        system.set_signal_handling(signal, SignalHandling::Ignore)?;
                    if initial_handling == SignalHandling::Ignore {
                        vacant.insert(SignalState {
                            user_state: UserSignalState::InitiallyIgnored,
                            internal_handler_enabled: false,
                        });
                        return Err(SetTrapError::InitiallyIgnored);
                    }
                }
                Entry::Vacant(vacant)
            }
            Entry::Occupied(mut occupied) => {
                if !override_ignore
                    && occupied.get().user_state == UserSignalState::InitiallyIgnored
                {
                    return Err(SetTrapError::InitiallyIgnored);
                }
                if occupied.get().internal_handler_enabled {
                    occupied.get_mut().user_state = UserSignalState::Trap(trap);
                    return Ok(());
                }
                Entry::Occupied(occupied)
            }
        };

        system.set_signal_handling(signal, (&trap).into())?;

        let state = SignalState {
            user_state: UserSignalState::Trap(trap),
            internal_handler_enabled: false,
        };
        #[allow(clippy::drop_ref)]
        match entry {
            Entry::Vacant(vacant) => drop(vacant.insert(state)),
            Entry::Occupied(mut occupied) => drop(occupied.insert(state)),
        }

        Ok(())
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
                let user_state = if previous_handler == SignalHandling::Ignore {
                    UserSignalState::InitiallyIgnored
                } else {
                    UserSignalState::Trap(Trap::default())
                };
                vacant.insert(SignalState {
                    user_state,
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
                system.set_signal_handling(Signal::SIGCHLD, (&state.user_state).into())?;
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
        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &Trap::Default);
    }

    #[test]
    fn setting_trap_to_ignore() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false)
            .unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &Trap::Ignore);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }

    #[test]
    fn setting_trap_to_command() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let trap = Trap::Command("echo".to_string());
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, trap.clone(), false)
            .unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &trap);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Catch
        );
    }

    #[test]
    fn setting_trap_to_default() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false)
            .unwrap();
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Default, false)
            .unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &Trap::Default);
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
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false);
        assert_eq!(result, Err(SetTrapError::InitiallyIgnored));

        // Idempotence
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false);
        assert_eq!(result, Err(SetTrapError::InitiallyIgnored));
    }

    #[test]
    fn resetting_trap_from_ignore_override() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, true)
            .unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &Trap::Ignore);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }

    #[test]
    fn setting_trap_for_two_signals() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .set_trap(&mut system, Signal::SIGUSR1, Trap::Ignore, false)
            .unwrap();
        let command = Trap::Command("echo".to_string());
        trap_set
            .set_trap(&mut system, Signal::SIGUSR2, command.clone(), false)
            .unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGUSR1), &Trap::Ignore);
        assert_eq!(trap_set.get_trap(Signal::SIGUSR2), &command);
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
        let result = trap_set.set_trap(&mut system, Signal::SIGKILL, Trap::Ignore, false);
        assert_eq!(result, Err(SetTrapError::SIGKILL));
    }

    #[test]
    fn setting_trap_for_sigstop() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let result = trap_set.set_trap(&mut system, Signal::SIGSTOP, Trap::Ignore, false);
        assert_eq!(result, Err(SetTrapError::SIGSTOP));
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
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false)
            .unwrap();
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Catch);
    }

    #[test]
    fn resetting_trap_from_ignore_no_override_after_enabling_internal_handler() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        let result = trap_set.set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false);
        assert_eq!(result, Err(SetTrapError::InitiallyIgnored));
    }

    #[test]
    fn resetting_trap_from_ignore_override_after_enabling_internal_handler() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, true)
            .unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &Trap::Ignore);
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
        trap_set
            .set_trap(&mut system, Signal::SIGCHLD, Trap::Ignore, false)
            .unwrap();
        trap_set.disable_internal_handlers(&mut system).unwrap();

        assert_eq!(trap_set.get_trap(Signal::SIGCHLD), &Trap::Ignore);
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Ignore
        );
    }
}
