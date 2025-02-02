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

//! Items that manage the state of a single signal

#[cfg(doc)]
use super::TrapSet;
use super::{Condition, SignalSystem};
use crate::system::{Disposition, Errno};
use std::collections::btree_map::{Entry, VacantEntry};
use std::rc::Rc;
use thiserror::Error;
use yash_syntax::source::Location;

/// Action performed when a [`Condition`] is met
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
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

impl From<&Action> for Disposition {
    fn from(trap: &Action) -> Self {
        match trap {
            Action::Default => Disposition::Default,
            Action::Ignore => Disposition::Ignore,
            Action::Command(_) => Disposition::Catch,
        }
    }
}

/// Error that may happen in [`TrapSet::set_action`]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SetActionError {
    /// Attempt to set a trap that has been ignored since the shell startup.
    #[error("the signal has been ignored since startup")]
    InitiallyIgnored,

    /// Attempt to set a trap for the `SIGKILL` signal.
    #[error("cannot set a trap for SIGKILL")]
    SIGKILL,

    /// Attempt to set a trap for the `SIGSTOP` signal.
    #[error("cannot set a trap for SIGSTOP")]
    SIGSTOP,

    /// Error from the underlying system interface.
    #[error(transparent)]
    SystemError(#[from] Errno),
}

/// Origin of the current trap action
///
/// The `Origin` enum indicates how the current trap action was set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Origin {
    /// The current trap action was inherited from the previous process that
    /// `exec`ed the shell.
    ///
    /// This is the default value.
    #[default]
    Inherited,

    /// The current trap action was set by the shell when entering a subshell.
    Subshell,

    /// The current trap action was set by the user.
    ///
    /// The location indicates the simple command that invoked the trap built-in
    /// that set the current action.
    User(Location),
}

/// State of the trap action for a condition
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrapState {
    /// Action taken when the condition is met
    pub action: Action,

    /// Origin of the current action
    pub origin: Origin,

    /// True iff a signal specified by the condition has been caught and the
    /// action command has not yet executed.
    pub pending: bool,
}

impl TrapState {
    fn from_initial_disposition(disposition: Disposition) -> Self {
        let action = match disposition {
            Disposition::Default => Action::Default,
            Disposition::Ignore => Action::Ignore,
            Disposition::Catch => panic!("initial disposition cannot be `Catch`"),
        };
        TrapState {
            action,
            origin: Origin::Inherited,
            pending: false,
        }
    }
}

/// Option for [`GrandState::enter_subshell`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnterSubshellOption {
    /// Keeps the current internal disposition configuration.
    KeepInternalDisposition,
    /// Resets the internal disposition configuration to the default.
    ClearInternalDisposition,
    /// Resets the internal disposition configuration to the default and sets the
    /// signal disposition to `Ignore`.
    Ignore,
}

/// Whole configuration and state for a trap condition
#[derive(Clone, Debug)]
pub struct GrandState {
    /// Current trap state
    current_state: TrapState,

    /// Trap state that was effective in the parent environment
    parent_state: Option<TrapState>,

    /// Current internal disposition
    ///
    /// The internal disposition is the signal disposition that is set by the
    /// shell itself, not by the user. This is used to handle some specific
    /// signals like `SIGCHLD` and `SIGTSTP`. When the user sets a trap for
    /// these signals, the actual signal disposition registered in the system
    /// is computed as the maximum of the user-defined disposition and the
    /// internal disposition.
    internal_disposition: Disposition,
}

impl GrandState {
    /// Returns the current trap state.
    #[inline]
    #[must_use]
    pub fn current_state(&self) -> &TrapState {
        &self.current_state
    }

    /// Returns the parent trap state.
    ///
    /// This is the trap state that was effective in the parent environment
    /// before the current environment was created as a subshell of the parent.
    /// Returns `None` if the current environment is not a subshell or the parent
    /// state was [cleared](Self::clear_parent_state).
    #[inline]
    #[must_use]
    pub fn parent_state(&self) -> Option<&TrapState> {
        self.parent_state.as_ref()
    }

    /// Clears the parent trap state.
    pub fn clear_parent_state(&mut self) {
        self.parent_state = None;
    }

    /// Inserts a new entry if the entry is vacant.
    ///
    /// If the condition is a signal, the new entry is initialized with the
    /// current signal disposition obtained from the system.
    pub fn insert_from_system_if_vacant<'a, S: SignalSystem>(
        system: &S,
        entry: Entry<'a, Condition, GrandState>,
    ) -> Result<&'a GrandState, Errno> {
        match entry {
            Entry::Vacant(vacant) => {
                let disposition = match *vacant.key() {
                    Condition::Signal(signal) => system.get_disposition(signal)?,
                    Condition::Exit => Disposition::Default,
                };
                let state = GrandState {
                    current_state: TrapState::from_initial_disposition(disposition),
                    parent_state: None,
                    internal_disposition: Disposition::Default,
                };
                Ok(vacant.insert(state))
            }

            Entry::Occupied(occupied) => Ok(occupied.into_mut()),
        }
    }

    /// Updates the entry with the new action.
    pub fn set_action<S: SignalSystem>(
        system: &mut S,
        entry: Entry<Condition, GrandState>,
        action: Action,
        origin: Location,
        override_ignore: bool,
    ) -> Result<(), SetActionError> {
        let cond = *entry.key();
        let disposition = (&action).into();
        let new_state = TrapState {
            action,
            origin: Origin::User(origin),
            pending: false,
        };

        match entry {
            Entry::Vacant(vacant) => {
                if let Condition::Signal(signal) = cond {
                    if !override_ignore {
                        let initial_disposition =
                            system.set_disposition(signal, Disposition::Ignore)?;
                        if initial_disposition == Disposition::Ignore {
                            vacant.insert(GrandState {
                                current_state: TrapState::from_initial_disposition(
                                    initial_disposition,
                                ),
                                parent_state: None,
                                internal_disposition: Disposition::Default,
                            });
                            return Err(SetActionError::InitiallyIgnored);
                        }
                    }

                    if override_ignore || disposition != Disposition::Ignore {
                        system.set_disposition(signal, disposition)?;
                    }
                }

                vacant.insert(GrandState {
                    current_state: new_state,
                    parent_state: None,
                    internal_disposition: Disposition::Default,
                });
            }

            Entry::Occupied(mut occupied) => {
                let state = occupied.get_mut();
                if !override_ignore
                    && state.current_state.action == Action::Ignore
                    && state.current_state.origin == Origin::Inherited
                {
                    return Err(SetActionError::InitiallyIgnored);
                }

                if let Condition::Signal(signal) = cond {
                    let internal = state.internal_disposition;
                    let old_disposition = internal.max((&state.current_state.action).into());
                    let new_disposition = internal.max(disposition);
                    if old_disposition != new_disposition {
                        system.set_disposition(signal, new_disposition)?;
                    }
                }

                state.current_state = new_state;
            }
        }

        Ok(())
    }

    /// Returns the current internal disposition.
    #[must_use]
    pub fn internal_disposition(&self) -> Disposition {
        self.internal_disposition
    }

    /// Sets the internal disposition.
    ///
    /// The condition of the given entry must be a signal, or this function
    /// panics.
    pub fn set_internal_disposition<S: SignalSystem>(
        system: &mut S,
        entry: Entry<Condition, GrandState>,
        disposition: Disposition,
    ) -> Result<(), Errno> {
        let signal = match *entry.key() {
            Condition::Signal(signal) => signal,
            Condition::Exit => panic!("exit condition cannot have an internal disposition"),
        };

        match entry {
            Entry::Vacant(_) if disposition == Disposition::Default => (),

            Entry::Vacant(vacant) => {
                let initial_disposition = system.set_disposition(signal, disposition)?;
                vacant.insert(GrandState {
                    current_state: TrapState::from_initial_disposition(initial_disposition),
                    parent_state: None,
                    internal_disposition: disposition,
                });
            }

            Entry::Occupied(mut occupied) => {
                let state = occupied.get_mut();
                let setting = (&state.current_state.action).into();
                let old_disposition = state.internal_disposition.max(setting);
                let new_disposition = disposition.max(setting);
                if old_disposition != new_disposition {
                    system.set_disposition(signal, new_disposition)?;
                }
                state.internal_disposition = disposition;
            }
        }

        Ok(())
    }

    /// Updates the trap states and gets ready for executing the body a
    /// subshell.
    ///
    /// If the current state has a user-specified command
    /// (`Action::Command(_)`), it is saved in the parent state and reset to the
    /// default. Additionally, the signal disposition is updated depending on the
    /// `option`.
    pub fn enter_subshell<S: SignalSystem>(
        &mut self,
        system: &mut S,
        cond: Condition,
        option: EnterSubshellOption,
    ) -> Result<(), Errno> {
        let old_setting = (&self.current_state.action).into();
        let old_disposition = self.internal_disposition.max(old_setting);

        if matches!(self.current_state.action, Action::Command(_)) {
            self.parent_state = Some(std::mem::replace(
                &mut self.current_state,
                TrapState {
                    action: Action::Default,
                    origin: Origin::Subshell,
                    pending: false,
                },
            ));
        }
        if option == EnterSubshellOption::Ignore {
            self.current_state.action = Action::Ignore;
        }

        let new_setting = (&self.current_state.action).into();
        let new_disposition = match option {
            EnterSubshellOption::KeepInternalDisposition => {
                self.internal_disposition.max(new_setting)
            }
            EnterSubshellOption::ClearInternalDisposition => new_setting,
            EnterSubshellOption::Ignore => Disposition::Ignore,
        };
        if old_disposition != new_disposition {
            if let Condition::Signal(signal) = cond {
                system.set_disposition(signal, new_disposition)?;
            }
        }
        self.internal_disposition = match option {
            EnterSubshellOption::KeepInternalDisposition => self.internal_disposition,
            EnterSubshellOption::ClearInternalDisposition | EnterSubshellOption::Ignore => {
                Disposition::Default
            }
        };
        Ok(())
    }

    /// Sets the disposition to `Ignore` for the given signal condition.
    ///
    /// This function creates a new `GrandState` entry with the current state
    /// having `Action::Ignore`. If the signal disposition is default, the shell
    /// sets the signal disposition to `Ignore` and the origin to `Subshell`. If
    /// the signal disposition is already `Ignore`, the origin is set to
    /// `Inherited` to disallow changing the action in a non-interactive shell.
    ///
    /// You should call this function in place of [`Self::enter_subshell`] if
    /// there is no entry for the condition yet.
    ///
    /// This function panics if the condition is not a signal.
    pub fn ignore<S: SignalSystem>(
        system: &mut S,
        vacant: VacantEntry<Condition, GrandState>,
    ) -> Result<(), Errno> {
        let signal = match *vacant.key() {
            Condition::Signal(signal) => signal,
            Condition::Exit => panic!("exit condition cannot be ignored"),
        };
        let initial_disposition = system.set_disposition(signal, Disposition::Ignore)?;
        let origin = match initial_disposition {
            Disposition::Default => Origin::Subshell,
            Disposition::Ignore => Origin::Inherited,
            Disposition::Catch => panic!("initial disposition cannot be `Catch`"),
        };
        vacant.insert(GrandState {
            current_state: TrapState {
                action: Action::Ignore,
                origin,
                pending: false,
            },
            parent_state: None,
            internal_disposition: Disposition::Default,
        });
        Ok(())
    }

    /// Marks this signal as caught.
    ///
    /// This function does nothing unless a user-specified trap action is set.
    pub fn mark_as_caught(&mut self) {
        self.current_state.pending = true;
    }

    /// Clears the mark of this signal being caught and returns the trap state.
    pub fn handle_if_caught(&mut self) -> Option<&TrapState> {
        if self.current_state.pending {
            self.current_state.pending = false;
            Some(&self.current_state)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::DummySystem;
    use super::*;
    use crate::system::r#virtual::{SIGCHLD, SIGQUIT, SIGTSTP, SIGTTOU, SIGUSR1};
    use assert_matches::assert_matches;
    use std::collections::BTreeMap;

    struct UnusedSystem;

    impl SignalSystem for UnusedSystem {
        fn signal_name_from_number(&self, number: crate::signal::Number) -> crate::signal::Name {
            unreachable!("signal_name_from_number({number})")
        }
        fn signal_number_from_name(
            &self,
            name: crate::signal::Name,
        ) -> Option<crate::signal::Number> {
            unreachable!("signal_number_from_name({name})")
        }
        fn get_disposition(&self, signal: crate::signal::Number) -> Result<Disposition, Errno> {
            unreachable!("get_disposition({signal})")
        }
        fn set_disposition(
            &mut self,
            signal: crate::signal::Number,
            disposition: Disposition,
        ) -> Result<Disposition, Errno> {
            unreachable!("set_disposition({signal}, {disposition:?})")
        }
    }

    #[test]
    fn insertion_with_default_inherited_disposition() {
        let system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let state = GrandState::insert_from_system_if_vacant(&system, entry).unwrap();
        assert_eq!(
            state.current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Inherited,
                pending: false,
            }
        );
        assert_eq!(state.parent_state(), None);
        assert_eq!(state.internal_disposition(), Disposition::Default);
    }

    #[test]
    fn insertion_with_inherited_disposition_of_ignore() {
        let mut system = DummySystem::default();
        system.0.insert(SIGCHLD, Disposition::Ignore);
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let state = GrandState::insert_from_system_if_vacant(&system, entry).unwrap();
        assert_eq!(
            state.current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::Inherited,
                pending: false,
            }
        );
        assert_eq!(state.parent_state(), None);
        assert_eq!(state.internal_disposition(), Disposition::Default);
    }

    #[test]
    fn insertion_with_occupied_entry() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("origin");
        let action = Action::Command("echo".into());
        let _ = GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false);

        // If the entry is occupied, the function should return the existing
        // state without accessing the system.
        let entry = map.entry(SIGCHLD.into());
        let state = GrandState::insert_from_system_if_vacant(&UnusedSystem, entry).unwrap();
        assert_eq!(
            state.current_state(),
            &TrapState {
                action,
                origin: Origin::User(origin),
                pending: false,
            }
        );
        assert_eq!(state.parent_state(), None);
        assert_eq!(state.internal_disposition(), Disposition::Default);
    }

    #[test]
    fn insertion_with_non_signal_condition() {
        let mut map = BTreeMap::new();
        let entry = map.entry(Condition::Exit);
        let state = GrandState::insert_from_system_if_vacant(&UnusedSystem, entry).unwrap();
        assert_eq!(
            state.current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Inherited,
                pending: false,
            }
        );
        assert_eq!(state.parent_state(), None);
        assert_eq!(state.internal_disposition(), Disposition::Default);
    }

    #[test]
    fn setting_trap_to_ignore_without_override_ignore() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("origin");

        let result =
            GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn setting_trap_to_ignore_with_override_ignore() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("origin");

        let result =
            GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), true);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn setting_trap_to_command() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let action = Action::Command("echo".into());
        let origin = Location::dummy("origin");

        let result =
            GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn setting_trap_to_default() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("foo");
        GrandState::set_action(&mut system, entry, Action::Ignore, origin, false).unwrap();

        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("bar");
        let result =
            GrandState::set_action(&mut system, entry, Action::Default, origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Default);
    }

    #[test]
    fn resetting_trap_from_ignore_no_override() {
        let mut system = DummySystem::default();
        system.0.insert(SIGCHLD, Disposition::Ignore);
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("foo");
        let result = GrandState::set_action(&mut system, entry, Action::Ignore, origin, false);
        assert_eq!(result, Err(SetActionError::InitiallyIgnored));

        // Idempotence
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("bar");
        let result = GrandState::set_action(&mut system, entry, Action::Ignore, origin, false);
        assert_eq!(result, Err(SetActionError::InitiallyIgnored));

        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn resetting_trap_from_ignore_override() {
        let mut system = DummySystem::default();
        system.0.insert(SIGCHLD, Disposition::Ignore);
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("origin");
        let result =
            GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), true);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn internal_disposition_ignore() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());

        let result = GrandState::set_internal_disposition(&mut system, entry, Disposition::Ignore);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].internal_disposition(),
            Disposition::Ignore
        );
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn internal_disposition_catch() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());

        let result = GrandState::set_internal_disposition(&mut system, entry, Disposition::Catch);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].internal_disposition(),
            Disposition::Catch
        );
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn action_ignore_and_internal_disposition_catch() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("origin");
        let _ = GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), false);
        let entry = map.entry(SIGCHLD.into());

        let result = GrandState::set_internal_disposition(&mut system, entry, Disposition::Catch);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].internal_disposition(),
            Disposition::Catch
        );
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn action_catch_and_internal_disposition_ignore() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGCHLD.into());
        let origin = Location::dummy("origin");
        let action = Action::Command("echo".into());
        let _ = GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false);
        let entry = map.entry(SIGCHLD.into());

        let result = GrandState::set_internal_disposition(&mut system, entry, Disposition::Ignore);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGCHLD.into()].internal_disposition(),
            Disposition::Ignore
        );
        assert_eq!(
            map[&SIGCHLD.into()].current_state(),
            &TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGCHLD.into()].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn set_internal_disposition_for_initially_defaulted_signal_then_allow_override() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let entry = map.entry(SIGTTOU.into());
        let _ = GrandState::set_internal_disposition(&mut system, entry, Disposition::Ignore);
        let entry = map.entry(SIGTTOU.into());
        let origin = Location::dummy("origin");
        let action = Action::Command("echo".into());

        let result =
            GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&SIGTTOU.into()].internal_disposition(),
            Disposition::Ignore
        );
        assert_eq!(
            map[&SIGTTOU.into()].current_state(),
            &TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&SIGTTOU.into()].parent_state(), None);
        assert_eq!(system.0[&SIGTTOU], Disposition::Catch);
    }

    #[test]
    fn set_internal_disposition_for_initially_ignored_signal_then_reject_override() {
        let mut system = DummySystem::default();
        system.0.insert(SIGTTOU, Disposition::Ignore);
        let mut map = BTreeMap::new();
        let cond = SIGTTOU.into();
        let entry = map.entry(cond);
        let _ = GrandState::set_internal_disposition(&mut system, entry, Disposition::Ignore);
        let entry = map.entry(cond);
        let origin = Location::dummy("origin");
        let action = Action::Command("echo".into());

        let result = GrandState::set_action(&mut system, entry, action, origin, false);
        assert_eq!(result, Err(SetActionError::InitiallyIgnored));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Ignore);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGTTOU], Disposition::Ignore);
    }

    #[test]
    fn enter_subshell_with_internal_disposition_keeping_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGCHLD.into();
        GrandState::set_internal_disposition(&mut system, map.entry(cond), Disposition::Catch)
            .unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::KeepInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Catch);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn enter_subshell_with_internal_disposition_clearing_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGCHLD.into();
        let entry = map.entry(cond);
        GrandState::set_internal_disposition(&mut system, entry, Disposition::Catch).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::ClearInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Default);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Default);
    }

    #[test]
    fn enter_subshell_with_ignore_and_no_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGCHLD.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), false).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::KeepInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Default);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn enter_subshell_with_ignore_clearing_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGCHLD.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        GrandState::set_action(&mut system, entry, Action::Ignore, origin.clone(), false).unwrap();
        let entry = map.entry(cond);
        GrandState::set_internal_disposition(&mut system, entry, Disposition::Catch).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::ClearInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Default);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::User(origin),
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn enter_subshell_with_command_and_no_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGCHLD.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::ClearInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Default);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Subshell,
                pending: false
            }
        );
        assert_eq!(
            map[&cond].parent_state(),
            Some(&TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            })
        );
        assert_eq!(system.0[&SIGCHLD], Disposition::Default);
    }

    #[test]
    fn enter_subshell_with_command_keeping_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGTSTP.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false).unwrap();
        let entry = map.entry(cond);
        GrandState::set_internal_disposition(&mut system, entry, Disposition::Ignore).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::KeepInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Ignore);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Subshell,
                pending: false
            }
        );
        assert_eq!(
            map[&cond].parent_state(),
            Some(&TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            })
        );
        assert_eq!(system.0[&SIGTSTP], Disposition::Ignore);
    }

    #[test]
    fn enter_subshell_with_command_clearing_internal_disposition() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGTSTP.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false).unwrap();
        let entry = map.entry(cond);
        GrandState::set_internal_disposition(&mut system, entry, Disposition::Ignore).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::ClearInternalDisposition,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Default);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Subshell,
                pending: false
            }
        );
        assert_eq!(
            map[&cond].parent_state(),
            Some(&TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            })
        );
        assert_eq!(system.0[&SIGTSTP], Disposition::Default);
    }

    #[test]
    fn enter_subshell_with_command_ignoring() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGQUIT.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false).unwrap();

        let result = map.get_mut(&cond).unwrap().enter_subshell(
            &mut system,
            cond,
            EnterSubshellOption::Ignore,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(map[&cond].internal_disposition(), Disposition::Default);
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::Subshell,
                pending: false
            }
        );
        assert_eq!(
            map[&cond].parent_state(),
            Some(&TrapState {
                action,
                origin: Origin::User(origin),
                pending: false
            })
        );
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
    }

    #[test]
    fn ignoring_initially_defaulted_signal() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGQUIT.into();
        let entry = map.entry(cond);
        let vacant = assert_matches!(entry, Entry::Vacant(vacant) => vacant);

        let result = GrandState::ignore(&mut system, vacant);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::Subshell,
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);

        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        let result = GrandState::set_action(&mut system, entry, action, origin, false);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn ignoring_initially_ignored_signal() {
        let mut system = DummySystem::default();
        system.0.insert(SIGQUIT, Disposition::Ignore);
        let mut map = BTreeMap::new();
        let cond = SIGQUIT.into();
        let entry = map.entry(cond);
        let vacant = assert_matches!(entry, Entry::Vacant(vacant) => vacant);

        let result = GrandState::ignore(&mut system, vacant);
        assert_eq!(result, Ok(()));
        assert_eq!(
            map[&cond].current_state(),
            &TrapState {
                action: Action::Ignore,
                origin: Origin::Inherited,
                pending: false
            }
        );
        assert_eq!(map[&cond].parent_state(), None);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);

        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        let result = GrandState::set_action(&mut system, entry, action, origin, false);
        assert_eq!(result, Err(SetActionError::InitiallyIgnored));
    }

    #[test]
    fn clearing_parent_setting() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGCHLD.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        GrandState::set_action(&mut system, entry, action, origin, false).unwrap();
        let state = map.get_mut(&cond).unwrap();
        state
            .enter_subshell(
                &mut system,
                cond,
                EnterSubshellOption::ClearInternalDisposition,
            )
            .unwrap();

        state.clear_parent_state();
        assert_eq!(
            state.current_state(),
            &TrapState {
                action: Action::Default,
                origin: Origin::Subshell,
                pending: false
            }
        );
        assert_eq!(state.parent_state(), None);
    }

    #[test]
    fn marking_as_caught_and_handling() {
        let mut system = DummySystem::default();
        let mut map = BTreeMap::new();
        let cond = SIGUSR1.into();
        let entry = map.entry(cond);
        let origin = Location::dummy("foo");
        let action = Action::Command("echo".into());
        GrandState::set_action(&mut system, entry, action.clone(), origin.clone(), false).unwrap();

        let state = &mut map.get_mut(&cond).unwrap();
        state.mark_as_caught();
        let expected_trap = TrapState {
            action,
            origin: Origin::User(origin),
            pending: true,
        };
        assert_eq!(state.current_state(), &expected_trap);
        assert_eq!(state.parent_state(), None);

        let trap = state.handle_if_caught();
        let expected_trap = TrapState {
            pending: false,
            ..expected_trap
        };
        assert_eq!(trap, Some(&expected_trap));

        let trap = state.handle_if_caught();
        assert_eq!(trap, None);
    }
}
