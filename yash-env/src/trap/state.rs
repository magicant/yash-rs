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

/// Error that may happen in [`TrapSet::set_action`].
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

    fn is_user_defined_command(&self) -> bool {
        matches!(
            self,
            Setting::UserSpecified(TrapState {
                action: Action::Command(_),
                ..
            })
        )
    }

    pub fn from_initial_disposition(disposition: Disposition) -> Self {
        match disposition {
            Disposition::Default | Disposition::Catch => Self::InitiallyDefaulted,
            Disposition::Ignore => Self::InitiallyIgnored,
        }
    }
}

impl From<&Setting> for Disposition {
    fn from(state: &Setting) -> Self {
        match state {
            Setting::InitiallyDefaulted => Disposition::Default,
            Setting::InitiallyIgnored => Disposition::Ignore,
            Setting::UserSpecified(trap) => (&trap.action).into(),
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
    /// Setting that is effective in the current environment
    current_setting: Setting,

    /// Setting that was effective in the parent environment
    parent_setting: Option<Setting>,

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

    /// Clears the parent trap state.
    pub fn clear_parent_setting(&mut self) {
        self.parent_setting = None;
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
        let setting = Setting::UserSpecified(TrapState {
            action,
            origin,
            pending: false,
        });
        let disposition = (&setting).into();

        match entry {
            Entry::Vacant(vacant) => {
                if let Condition::Signal(signal) = cond {
                    if !override_ignore {
                        let initial_disposition =
                            system.set_disposition(signal, Disposition::Ignore)?;
                        if initial_disposition == Disposition::Ignore {
                            vacant.insert(GrandState {
                                current_setting: Setting::InitiallyIgnored,
                                parent_setting: None,
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
                    current_setting: setting,
                    parent_setting: None,
                    internal_disposition: Disposition::Default,
                });
            }

            Entry::Occupied(mut occupied) => {
                let state = occupied.get_mut();
                if !override_ignore && state.current_setting == Setting::InitiallyIgnored {
                    return Err(SetActionError::InitiallyIgnored);
                }

                if let Condition::Signal(signal) = cond {
                    let internal = state.internal_disposition;
                    let old_disposition = internal.max((&state.current_setting).into());
                    let new_disposition = internal.max(disposition);
                    if old_disposition != new_disposition {
                        system.set_disposition(signal, new_disposition)?;
                    }
                }

                state.current_setting = setting;
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
                    current_setting: Setting::from_initial_disposition(initial_disposition),
                    parent_setting: None,
                    internal_disposition: disposition,
                });
            }

            Entry::Occupied(mut occupied) => {
                let state = occupied.get_mut();
                let setting = (&state.current_setting).into();
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
        let old_setting = (&self.current_setting).into();
        let old_disposition = self.internal_disposition.max(old_setting);

        if self.current_setting.is_user_defined_command() {
            self.parent_setting = Some(std::mem::replace(
                &mut self.current_setting,
                Setting::InitiallyDefaulted,
            ));
        }

        let new_setting = (&self.current_setting).into();
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
    /// This function creates a new entry having `Setting::InitiallyDefaulted`
    /// or `Setting::InitiallyIgnored` based on the current setting.
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
        vacant.insert(GrandState {
            current_setting: Setting::from_initial_disposition(initial_disposition),
            parent_setting: None,
            internal_disposition: Disposition::Default,
        });
        Ok(())
    }

    /// Marks this signal as caught.
    ///
    /// This function does nothing unless a user-specified trap action is set.
    pub fn mark_as_caught(&mut self) {
        if let Setting::UserSpecified(state) = &mut self.current_setting {
            state.pending = true;
        }
    }

    /// Clears the mark of this signal being caught and returns the trap state.
    pub fn handle_if_caught(&mut self) -> Option<&TrapState> {
        match &mut self.current_setting {
            Setting::UserSpecified(trap) if trap.pending => {
                trap.pending = false;
                Some(trap)
            }
            _ => None,
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
            map[&SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
            map[&SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
            map[&SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
            map[&SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Default,
                    origin,
                    pending: false
                }),
                None
            )
        );
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

        assert_eq!(map[&SIGCHLD.into()].get_state(), (None, None));
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
            map[&SIGCHLD.into()].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
        assert_eq!(map[&SIGCHLD.into()].get_state(), (None, None));
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
        assert_eq!(map[&SIGCHLD.into()].get_state(), (None, None));
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
        assert_matches!(map[&SIGCHLD.into()].get_state(), (Some(state), None) => {
            assert_eq!(state.action, Action::Ignore);
            assert_eq!(state.origin, origin);
        });
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
        assert_matches!(map[&SIGCHLD.into()].get_state(), (Some(state), None) => {
            assert_eq!(state.action, action);
            assert_eq!(state.origin, origin);
        });
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
            map[&SIGTTOU.into()].get_state(),
            (
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
        assert_eq!(map[&cond].get_state(), (None, None));
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
        assert_eq!(map[&cond].get_state(), (None, None));
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
        assert_eq!(
            map[&SIGCHLD.into()].internal_disposition(),
            Disposition::Default
        );
        assert_eq!(map[&cond].get_state(), (None, None));
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
            map[&cond].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
            map[&cond].get_state(),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin,
                    pending: false
                }),
                None
            )
        );
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
            map[&cond].get_state(),
            (
                None,
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
            )
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
            map[&cond].get_state(),
            (
                None,
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
            )
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
            map[&cond].get_state(),
            (
                None,
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
            )
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
            map[&cond].get_state(),
            (
                None,
                Some(&TrapState {
                    action,
                    origin,
                    pending: false
                }),
            )
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
        assert_eq!(map[&cond].get_state(), (None, None));
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
        assert_eq!(map[&cond].get_state(), (None, None));
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

        state.clear_parent_setting();
        assert_eq!(state.get_state(), (None, None));
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
            origin,
            pending: true,
        };
        assert_eq!(state.get_state(), (Some(&expected_trap), None));

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
