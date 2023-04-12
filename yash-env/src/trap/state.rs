// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Items that manage the state of a single signal.

use super::cond::Condition;
use super::SignalSystem;
#[cfg(doc)]
use super::TrapSet;
use crate::system::{Errno, SignalHandling};
use std::{collections::btree_map::Entry, rc::Rc};
use yash_syntax::source::Location;

/// Action performed when a [`Condition`] is met
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Action {
    /// Performs the default action.
    ///
    /// For signal conditions, the behavior depends on the signal delivered.
    /// For other conditions, this is equivalent to `Ignore`.
    #[default]
    Default,

    /// Pretends as if the condition was not met.
    Ignore,

    /// Executes a command string.
    Command(Rc<str>),
}

impl From<&Action> for SignalHandling {
    fn from(trap: &Action) -> Self {
        match trap {
            Action::Default => SignalHandling::Default,
            Action::Ignore => SignalHandling::Ignore,
            Action::Command(_) => SignalHandling::Catch,
        }
    }
}

/// Error that may happen in [`TrapSet::set_action`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetActionError {
    /// Attempt to set a trap that has been ignored since the shell startup.
    InitiallyIgnored,
    /// Attempt to set a trap for the `SIGKILL` signal.
    SIGKILL,
    /// Attempt to set a trap for the `SIGSTOP` signal.
    SIGSTOP,
    /// Error from the underlying system interface.
    SystemError(Errno),
}

impl std::fmt::Display for SetActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use SetActionError::*;
        match self {
            InitiallyIgnored => "the signal has been ignored since startup".fmt(f),
            SIGKILL => "cannot set a trap for SIGKILL".fmt(f),
            SIGSTOP => "cannot set a trap for SIGSTOP".fmt(f),
            SystemError(errno) => errno.fmt(f),
        }
    }
}

impl std::error::Error for SetActionError {}

impl From<Errno> for SetActionError {
    fn from(errno: Errno) -> Self {
        SetActionError::SystemError(errno)
    }
}

/// State of the trap action for a condition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrapState {
    /// Action taken when the condition is met.
    pub action: Action,
    /// Location of the simple command that invoked the trap built-in that set
    /// the current action.
    pub origin: Location,
    /// True iff a signal specified by the condition has been caught and the
    /// action command has not yet executed.
    pub pending: bool,
}

/// User-visible trap setting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Setting {
    /// The user has not yet set a trap for the signal specified by the
    /// condition, and the signal disposition the shell has inherited from the
    /// pre-exec process is `SIG_DFL`.
    InitiallyDefaulted,
    /// The user has not yet set a trap for the signal specified by the
    /// condition, and the signal disposition the shell has inherited from the
    /// pre-exec process is `SIG_IGN`.
    InitiallyIgnored,
    /// User-defined trap.
    UserSpecified(TrapState),
}

impl Setting {
    pub fn as_trap(&self) -> Option<&TrapState> {
        if let Setting::UserSpecified(trap) = self {
            Some(trap)
        } else {
            None
        }
    }

    pub fn from_initial_handling(handling: SignalHandling) -> Self {
        match handling {
            SignalHandling::Default | SignalHandling::Catch => Self::InitiallyDefaulted,
            SignalHandling::Ignore => Self::InitiallyIgnored,
        }
    }
}

impl From<&Setting> for SignalHandling {
    fn from(state: &Setting) -> Self {
        match state {
            Setting::InitiallyDefaulted => SignalHandling::Default,
            Setting::InitiallyIgnored => SignalHandling::Ignore,
            Setting::UserSpecified(trap) => (&trap.action).into(),
        }
    }
}

/// Whole configuration and state for a trap condition.
#[derive(Clone, Debug)]
pub struct GrandState {
    /// Setting that is effective in the current environment.
    pub current_setting: Setting,

    /// Setting that was effective in the parent environment.
    pub parent_setting: Option<Setting>,

    /// Whether the internal handler has been installed in the current environment.
    pub internal_handler_enabled: bool,
}

impl GrandState {
    /// Returns the current and parent trap states.
    ///
    /// This function returns a pair of optional trap states. The first is the
    /// currently configured trap action, and the second is the action set
    /// before [`enter_subshell`](Self::enter_subshell) was called.
    ///
    /// This function does not reflect the initial signal actions the shell
    /// inherited on startup.
    #[must_use]
    pub fn get_state(&self) -> (Option<&TrapState>, Option<&TrapState>) {
        let current = self.current_setting.as_trap();
        let parent = self.parent_setting.as_ref().and_then(Setting::as_trap);
        (current, parent)
    }

    /// Updates the entry with the new action.
    pub fn set_action<S: SignalSystem>(
        system: &mut S,
        entry: Entry<Condition, GrandState>,
        action: Action,
        origin: Location,
        override_ignore: bool,
    ) -> Result<(), SetActionError> {
        let state = TrapState {
            action,
            origin,
            pending: false,
        };
        let cond = *entry.key();

        let entry = match entry {
            Entry::Vacant(vacant) => {
                if let Condition::Signal(signal) = cond {
                    if !override_ignore {
                        let initial_handling =
                            system.set_signal_handling(signal, SignalHandling::Ignore)?;
                        if initial_handling == SignalHandling::Ignore {
                            vacant.insert(GrandState {
                                current_setting: Setting::InitiallyIgnored,
                                parent_setting: None,
                                internal_handler_enabled: false,
                            });
                            return Err(SetActionError::InitiallyIgnored);
                        }
                    }
                }
                Entry::Vacant(vacant)
            }
            Entry::Occupied(mut occupied) => {
                if !override_ignore && occupied.get().current_setting == Setting::InitiallyIgnored {
                    return Err(SetActionError::InitiallyIgnored);
                }
                if occupied.get().internal_handler_enabled {
                    //TODO
                    occupied.get_mut().current_setting = Setting::UserSpecified(state);
                    return Ok(());
                }
                Entry::Occupied(occupied)
            }
        };

        if let Condition::Signal(signal) = cond {
            system.set_signal_handling(signal, (&state.action).into())?;
        }

        let state = GrandState {
            current_setting: Setting::UserSpecified(state),
            parent_setting: None,
            internal_handler_enabled: false,
        };
        #[allow(clippy::drop_ref)]
        match entry {
            Entry::Vacant(vacant) => drop(vacant.insert(state)),
            Entry::Occupied(mut occupied) => drop(occupied.insert(state)),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::cond::Signal;
    use super::super::tests::DummySystem;
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn setting_trap_to_ignore() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(Signal::SIGCHLD.into());
        let origin = Location::dummy("origin");

        let result =
            GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&Signal::SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Ignore);
    }

    #[test]
    fn setting_trap_to_command() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(Signal::SIGCHLD.into());
        let action = Action::Command("echo".into());
        let origin = Location::dummy("origin");

        let result =
            GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&Signal::SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
                None
            )
        );
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Catch);
    }

    #[test]
    fn setting_trap_to_default() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(Signal::SIGCHLD.into());
        let origin = Location::dummy("foo");
        GrandState::set_action(&mut system, entry, Action::Ignore, origin, false).unwrap();

        let entry = map.entry(Signal::SIGCHLD.into());
        let origin = Location::dummy("bar");
        let result =
            GrandState::set_action(&mut system, entry, Action::Default, origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&Signal::SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Default,
                    origin,
                    pending: false
                }),
                None
            )
        );
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Default);
    }

    #[test]
    fn resetting_trap_from_ignore_no_override() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut map = BTreeMap::new();
        let entry = map.entry(Signal::SIGCHLD.into());
        let origin = Location::dummy("foo");
        let result = GrandState::set_action(&mut system, entry, Action::Ignore, origin, false);
        assert_eq!(result, Err(SetActionError::InitiallyIgnored));

        // Idempotence
        let entry = map.entry(Signal::SIGCHLD.into());
        let origin = Location::dummy("bar");
        let result = GrandState::set_action(&mut system, entry, Action::Ignore, origin, false);
        assert_eq!(result, Err(SetActionError::InitiallyIgnored));

        assert_eq!(map[&Signal::SIGCHLD.into()].get_state(), (None, None));
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Ignore);
    }

    #[test]
    fn resetting_trap_from_ignore_override() {
        let mut system = DummySystem::default();
        system.0.insert(Signal::SIGCHLD, SignalHandling::Ignore);
        let mut map = BTreeMap::new();
        let entry = map.entry(Signal::SIGCHLD.into());
        let origin = Location::dummy("origin");
        let result =
            GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), true);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&Signal::SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
        assert_eq!(system.0[&Signal::SIGCHLD], SignalHandling::Ignore);
    }
}
