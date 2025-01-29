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

//! Signal and other event handling settings.
//!
//! The trap is a mechanism of the shell that allows you to configure event
//! handlers for specific situations. A [`TrapSet`] is a mapping from [`Condition`]s to
//! [`Action`]s. When the mapping is modified, it updates the corresponding signal
//! disposition in the underlying system through a [`SignalSystem`] implementor.
//! Methods of `TrapSet` expect they are passed the same system instance in
//! every call to keep it in a correct state.
//!
//! `TrapSet` manages two types of signal handling configurations. One is
//! user-defined traps, which the user explicitly configures with the trap
//! built-in. The other is internal dispositions, which the shell implicitly
//! installs to the system to implement additional actions it needs to perform.
//! `TrapSet` merges the two configurations into a single [`Disposition`] for
//! each signal and sets it to the system.
//!
//! No signal disposition is involved for conditions other than signals, and the
//! trap set serves only as a storage for action settings.

mod cond;
mod state;

pub use self::cond::Condition;
pub use self::state::{Action, SetActionError, TrapState};
use self::state::{EnterSubshellOption, GrandState};
use crate::signal;
use crate::system::{Disposition, Errno};
#[cfg(doc)]
use crate::system::{SharedSystem, System};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use yash_syntax::source::Location;

/// System interface for signal handling configuration
pub trait SignalSystem {
    /// Returns the name of a signal from its number.
    #[must_use]
    fn signal_name_from_number(&self, number: signal::Number) -> signal::Name;

    /// Returns the signal number from its name.
    ///
    /// This function returns the signal number corresponding to the signal name
    /// in the system. If the signal name is not supported, it returns `None`.
    ///
    /// Note that the `TrapSet` implementation assumes that the system supports
    /// all the following signals:
    ///
    /// - `SIGCHLD`
    /// - `SIGINT`
    /// - `SIGTERM`
    /// - `SIGQUIT`
    /// - `SIGTSTP`
    /// - `SIGTTIN`
    /// - `SIGTTOU`
    ///
    /// If this method returns `None` for any of these signals, `TrapSet` will
    /// panic.
    #[must_use]
    fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number>;

    /// Returns the current disposition for a signal.
    ///
    /// This function returns the current disposition for the specified signal like
    /// [`set_disposition`](Self::set_disposition) does, but does not change the
    /// disposition.
    fn get_disposition(&self, signal: signal::Number) -> Result<Disposition, Errno>;

    /// Sets how a signal is handled.
    ///
    /// This function updates the signal blocking mask and the disposition for
    /// the specified signal, and returns the previous disposition.
    fn set_disposition(
        &mut self,
        signal: signal::Number,
        disposition: Disposition,
    ) -> Result<Disposition, Errno>;
}

/// Iterator of trap actions configured in a [trap set](TrapSet).
///
/// [`TrapSet::iter`] returns this type of iterator.
#[must_use]
pub struct Iter<'a> {
    inner: std::collections::btree_map::Iter<'a, Condition, GrandState>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a Condition, Option<&'a TrapState>, Option<&'a TrapState>);

    fn next(&mut self) -> Option<(&'a Condition, Option<&'a TrapState>, Option<&'a TrapState>)> {
        loop {
            let (cond, state) = self.inner.next()?;
            let (current, parent) = state.get_state();
            if current.is_some() || parent.is_some() {
                return Some((cond, current, parent));
            }
        }
    }
}

/// Collection of event handling settings.
///
/// See the [module documentation](self) for details.
#[derive(Clone, Debug, Default)]
pub struct TrapSet {
    traps: BTreeMap<Condition, GrandState>,
}

impl TrapSet {
    /// Returns the current state for a condition.
    ///
    /// This function returns a pair of optional trap states. The first is the
    /// currently configured trap action, and the second is the action set
    /// before [`enter_subshell`](Self::enter_subshell) was called.
    ///
    /// This function does not reflect the initial signal actions the shell
    /// inherited on startup.
    pub fn get_state<C: Into<Condition>>(
        &self,
        cond: C,
    ) -> (Option<&TrapState>, Option<&TrapState>) {
        self.get_state_impl(cond.into())
    }

    fn get_state_impl(&self, cond: Condition) -> (Option<&TrapState>, Option<&TrapState>) {
        match self.traps.get(&cond) {
            None => (None, None),
            Some(state) => state.get_state(),
        }
    }

    /// Sets a trap action for a condition.
    ///
    /// If the condition is a signal, this function installs a signal handler to
    /// the specified underlying system.
    ///
    /// If `override_ignore` is `false`, you cannot set a trap for a signal that
    /// has been ignored since the shell startup. An interactive shell should
    /// set `override_ignore` to `true` to bypass this restriction.
    ///
    /// You can never set a trap for `SIGKILL` or `SIGSTOP`.
    ///
    /// `origin` should be the location of the command performing this trap
    /// update. It is only informative: It does not affect the signal handling
    /// behavior and can be referenced later by [`get_state`](Self::get_state).
    ///
    /// This function clears all parent states remembered when [entering a
    /// subshell](Self::enter_subshell), not only for the specified condition
    /// but also for all other conditions.
    pub fn set_action<S: SignalSystem, C: Into<Condition>>(
        &mut self,
        system: &mut S,
        cond: C,
        action: Action,
        origin: Location,
        override_ignore: bool,
    ) -> Result<(), SetActionError> {
        self.set_action_impl(system, cond.into(), action, origin, override_ignore)
    }

    fn set_action_impl<S: SignalSystem>(
        &mut self,
        system: &mut S,
        cond: Condition,
        action: Action,
        origin: Location,
        override_ignore: bool,
    ) -> Result<(), SetActionError> {
        if let Condition::Signal(number) = cond {
            match system.signal_name_from_number(number) {
                signal::Name::Kill => return Err(SetActionError::SIGKILL),
                signal::Name::Stop => return Err(SetActionError::SIGSTOP),
                _ => {}
            }
        }

        self.clear_parent_settings();

        let entry = self.traps.entry(cond);
        GrandState::set_action(system, entry, action, origin, override_ignore)
    }

    fn clear_parent_settings(&mut self) {
        for state in self.traps.values_mut() {
            state.clear_parent_setting();
        }
    }

    /// Returns an iterator over the signal actions configured in this trap set.
    ///
    /// The iterator yields tuples of the signal, the currently configured trap
    /// action, and the action set before
    /// [`enter_subshell`](Self::enter_subshell) was called.
    #[inline]
    pub fn iter(&self) -> Iter<'_> {
        let inner = self.traps.iter();
        Iter { inner }
    }

    /// Updates signal dispositions on entering a subshell.
    ///
    /// ## Resetting non-ignore traps
    ///
    /// POSIX requires that traps other than `Action::Ignore` be reset when
    /// entering a subshell. This function achieves that effect.
    ///
    /// The trap set will remember the original trap states as the parent
    /// states. You can get them from the second return value of
    /// [`get_state`](Self::get_state) or the third item of tuples yielded by an
    /// [iterator](Self::iter).
    ///
    /// Note that trap actions other than `Trap::Command` remain as before.
    ///
    /// ## Clearing internal dispositions
    ///
    /// Internal dispositions that have been installed are cleared except for
    /// the SIGCHLD signal.
    ///
    /// ## Ignoring SIGINT and SIGQUIT
    ///
    /// If `ignore_sigint_sigquit` is true, this function sets the dispositions
    /// for SIGINT and SIGQUIT to `Ignore`.
    ///
    /// ## Ignoring SIGTSTP, SIGTTIN, and SIGTTOU
    ///
    /// If `keep_internal_dispositions_for_stoppers` is true and the internal
    /// dispositions have been [enabled for SIGTSTP, SIGTTIN, and
    /// SIGTTOU](Self::enable_internal_dispositions_for_stoppers), this function
    /// leaves the dispositions for those signals set to `Ignore`.
    ///
    /// ## Errors
    ///
    /// This function ignores any errors that may occur when setting signal
    /// dispositions.
    pub fn enter_subshell<S: SignalSystem>(
        &mut self,
        system: &mut S,
        ignore_sigint_sigquit: bool,
        keep_internal_dispositions_for_stoppers: bool,
    ) {
        self.clear_parent_settings();

        for (&cond, state) in &mut self.traps {
            let option = match cond {
                Condition::Exit => EnterSubshellOption::ClearInternalDisposition,
                Condition::Signal(number) => {
                    use signal::Name::*;
                    match system.signal_name_from_number(number) {
                        Chld => EnterSubshellOption::KeepInternalDisposition,
                        Int | Quit if ignore_sigint_sigquit => EnterSubshellOption::Ignore,
                        Tstp | Ttin | Ttou
                            if keep_internal_dispositions_for_stoppers
                                && state.internal_disposition() != Disposition::Default =>
                        {
                            EnterSubshellOption::Ignore
                        }
                        _ => EnterSubshellOption::ClearInternalDisposition,
                    }
                }
            };
            _ = state.enter_subshell(system, cond, option);
        }

        if ignore_sigint_sigquit {
            for name in [signal::Name::Int, signal::Name::Quit] {
                let number = system
                    .signal_number_from_name(name)
                    .unwrap_or_else(|| panic!("missing support for signal {name}"));
                match self.traps.entry(Condition::Signal(number)) {
                    Entry::Vacant(vacant) => _ = GrandState::ignore(system, vacant),
                    // If the entry is occupied, the signal is already ignored in the loop above.
                    Entry::Occupied(_) => {}
                }
            }
        }
    }

    /// Sets the `pending` flag of the [`TrapState`] for the specified signal.
    ///
    /// This function does nothing if no trap action has been
    /// [set](Self::set_action) for the signal.
    pub fn catch_signal(&mut self, signal: signal::Number) {
        if let Some(state) = self.traps.get_mut(&Condition::Signal(signal)) {
            state.mark_as_caught();
        }
    }

    /// Resets the `pending` flag of the [`TrapState`] for the specified signal.
    ///
    /// Returns the [`TrapState`] if the flag was set.
    pub fn take_signal_if_caught(&mut self, signal: signal::Number) -> Option<&TrapState> {
        self.traps
            .get_mut(&signal.into())
            .and_then(|state| state.handle_if_caught())
    }

    /// Returns a signal that has been [caught](Self::catch_signal).
    ///
    /// This function clears the `pending` flag of the [`TrapState`] for the
    /// specified signal.
    ///
    /// If there is more than one caught signal, it is unspecified which one of
    /// them is returned. If there is no caught signal, `None` is returned.
    pub fn take_caught_signal(&mut self) -> Option<(signal::Number, &TrapState)> {
        self.traps.iter_mut().find_map(|(&cond, state)| match cond {
            Condition::Signal(signal) => state.handle_if_caught().map(|trap| (signal, trap)),
            _ => None,
        })
    }

    fn set_internal_disposition<S: SignalSystem>(
        &mut self,
        signal: signal::Name,
        disposition: Disposition,
        system: &mut S,
    ) -> Result<(), Errno> {
        let number = system
            .signal_number_from_name(signal)
            .unwrap_or_else(|| panic!("missing support for signal {signal}"));
        let entry = self.traps.entry(Condition::Signal(number));
        GrandState::set_internal_disposition(system, entry, disposition)
    }

    /// Installs the internal disposition for `SIGCHLD`.
    ///
    /// You should install the internal disposition for `SIGCHLD` by using this
    /// function before waiting for `SIGCHLD` with [`System::wait`] and
    /// [`SharedSystem::wait_for_signal`]. The disposition allows catching
    /// `SIGCHLD`.
    ///
    /// This function remembers that the disposition has been installed, so a
    /// second call to the function will be a no-op.
    pub fn enable_internal_disposition_for_sigchld<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        self.set_internal_disposition(signal::Name::Chld, Disposition::Catch, system)
    }

    /// Installs the internal dispositions for `SIGINT`, `SIGTERM`, and `SIGQUIT`.
    ///
    /// An interactive shell should install the internal dispositions for these
    /// signals by using this function. The dispositions catch `SIGINT` and
    /// ignore `SIGTERM` and `SIGQUIT`.
    ///
    /// This function remembers that the dispositions have been installed, so a
    /// second call to the function will be a no-op.
    pub fn enable_internal_dispositions_for_terminators<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        self.set_internal_disposition(signal::Name::Int, Disposition::Catch, system)?;
        self.set_internal_disposition(signal::Name::Term, Disposition::Ignore, system)?;
        self.set_internal_disposition(signal::Name::Quit, Disposition::Ignore, system)
    }

    /// Installs the internal dispositions for `SIGTSTP`, `SIGTTIN`, and `SIGTTOU`.
    ///
    /// An interactive job-controlling shell should install the internal
    /// dispositions for these signals by using this function. The dispositions
    /// ignore the signals.
    ///
    /// This function remembers that the dispositions have been installed, so a
    /// second call to the function will be a no-op.
    pub fn enable_internal_dispositions_for_stoppers<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        self.set_internal_disposition(signal::Name::Tstp, Disposition::Ignore, system)?;
        self.set_internal_disposition(signal::Name::Ttin, Disposition::Ignore, system)?;
        self.set_internal_disposition(signal::Name::Ttou, Disposition::Ignore, system)
    }

    /// Uninstalls the internal dispositions for `SIGINT`, `SIGTERM`, and `SIGQUIT`.
    pub fn disable_internal_dispositions_for_terminators<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        self.set_internal_disposition(signal::Name::Int, Disposition::Default, system)?;
        self.set_internal_disposition(signal::Name::Term, Disposition::Default, system)?;
        self.set_internal_disposition(signal::Name::Quit, Disposition::Default, system)
    }

    /// Uninstalls the internal dispositions for `SIGTSTP`, `SIGTTIN`, and `SIGTTOU`.
    pub fn disable_internal_dispositions_for_stoppers<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        self.set_internal_disposition(signal::Name::Tstp, Disposition::Default, system)?;
        self.set_internal_disposition(signal::Name::Ttin, Disposition::Default, system)?;
        self.set_internal_disposition(signal::Name::Ttou, Disposition::Default, system)
    }

    /// Uninstalls all internal dispositions.
    ///
    /// This function removes all internal dispositions that have been
    /// previously installed by `self`, except for the `SIGCHLD` signal.
    /// Dispositions for any existing user-defined traps are not affected.
    pub fn disable_internal_dispositions<S: SignalSystem>(
        &mut self,
        system: &mut S,
    ) -> Result<(), Errno> {
        self.set_internal_disposition(signal::Name::Chld, Disposition::Default, system)?;
        self.disable_internal_dispositions_for_terminators(system)?;
        self.disable_internal_dispositions_for_stoppers(system)
    }
}

impl<'a> IntoIterator for &'a TrapSet {
    type Item = (&'a Condition, Option<&'a TrapState>, Option<&'a TrapState>);
    type IntoIter = Iter<'a>;

    #[inline(always)]
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::ProcessState;
    use crate::system::r#virtual::VirtualSystem;
    use crate::system::r#virtual::{
        SIGCHLD, SIGINT, SIGKILL, SIGQUIT, SIGSTOP, SIGTERM, SIGTSTP, SIGTTIN, SIGTTOU, SIGUSR1,
        SIGUSR2,
    };
    use crate::system::System as _;
    use crate::system::SystemEx as _;
    use crate::tests::in_virtual_system;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct DummySystem(pub HashMap<signal::Number, Disposition>);

    impl SignalSystem for DummySystem {
        fn signal_name_from_number(&self, number: signal::Number) -> signal::Name {
            VirtualSystem::new().signal_name_from_number(number)
        }

        fn signal_number_from_name(&self, name: signal::Name) -> Option<signal::Number> {
            VirtualSystem::new().signal_number_from_name(name)
        }

        fn get_disposition(&self, signal: signal::Number) -> Result<Disposition, Errno> {
            Ok(self.0.get(&signal).copied().unwrap_or_default())
        }

        fn set_disposition(
            &mut self,
            signal: signal::Number,
            disposition: Disposition,
        ) -> Result<Disposition, Errno> {
            Ok(self.0.insert(signal, disposition).unwrap_or_default())
        }
    }

    #[test]
    fn default_trap() {
        let trap_set = TrapSet::default();
        assert_eq!(trap_set.get_state(SIGCHLD), (None, None));
    }

    #[test]
    fn setting_trap_for_two_signals() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        let result = trap_set.set_action(
            &mut system,
            SIGUSR1,
            Action::Ignore,
            origin_1.clone(),
            false,
        );
        assert_eq!(result, Ok(()));

        let command = Action::Command("echo".into());
        let origin_2 = Location::dummy("bar");
        let result = trap_set.set_action(
            &mut system,
            SIGUSR2,
            command.clone(),
            origin_2.clone(),
            false,
        );
        assert_eq!(result, Ok(()));

        assert_eq!(
            trap_set.get_state(SIGUSR1),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin: Some(origin_1),
                    pending: false
                }),
                None
            )
        );
        assert_eq!(
            trap_set.get_state(SIGUSR2),
            (
                Some(&TrapState {
                    action: command,
                    origin: Some(origin_2),
                    pending: false
                }),
                None
            )
        );
        assert_eq!(system.0[&SIGUSR1], Disposition::Ignore);
        assert_eq!(system.0[&SIGUSR2], Disposition::Catch);
    }

    #[test]
    fn setting_trap_for_sigkill() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        let result = trap_set.set_action(&mut system, SIGKILL, Action::Ignore, origin, false);
        assert_eq!(result, Err(SetActionError::SIGKILL));
        assert_eq!(trap_set.get_state(SIGKILL), (None, None));
        assert_eq!(system.0.get(&SIGKILL), None);
    }

    #[test]
    fn setting_trap_for_sigstop() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        let result = trap_set.set_action(&mut system, SIGSTOP, Action::Ignore, origin, false);
        assert_eq!(result, Err(SetActionError::SIGSTOP));
        assert_eq!(trap_set.get_state(SIGSTOP), (None, None));
        assert_eq!(system.0.get(&SIGSTOP), None);
    }

    #[test]
    fn basic_iteration() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        trap_set
            .set_action(
                &mut system,
                SIGUSR1,
                Action::Ignore,
                origin_1.clone(),
                false,
            )
            .unwrap();
        let command = Action::Command("echo".into());
        let origin_2 = Location::dummy("bar");
        trap_set
            .set_action(
                &mut system,
                SIGUSR2,
                command.clone(),
                origin_2.clone(),
                false,
            )
            .unwrap();

        let mut i = trap_set.iter();
        let first = i.next().unwrap();
        assert_eq!(first.0, &Condition::Signal(SIGUSR1));
        assert_eq!(first.1.unwrap().action, Action::Ignore);
        assert_eq!(first.1.unwrap().origin, Some(origin_1));
        assert_eq!(first.2, None);
        let second = i.next().unwrap();
        assert_eq!(second.0, &Condition::Signal(SIGUSR2));
        assert_eq!(second.1.unwrap().action, command);
        assert_eq!(second.1.unwrap().origin, Some(origin_2));
        assert_eq!(first.2, None);
        assert_eq!(i.next(), None);
    }

    #[test]
    fn iteration_after_entering_subshell() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        trap_set
            .set_action(
                &mut system,
                SIGUSR1,
                Action::Ignore,
                origin_1.clone(),
                false,
            )
            .unwrap();
        let command = Action::Command("echo".into());
        let origin_2 = Location::dummy("bar");
        trap_set
            .set_action(
                &mut system,
                SIGUSR2,
                command.clone(),
                origin_2.clone(),
                false,
            )
            .unwrap();
        trap_set.enter_subshell(&mut system, false, false);

        let mut i = trap_set.iter();
        let first = i.next().unwrap();
        assert_eq!(first.0, &Condition::Signal(SIGUSR1));
        assert_eq!(first.1.unwrap().action, Action::Ignore);
        assert_eq!(first.1.unwrap().origin, Some(origin_1));
        assert_eq!(first.2, None);
        let second = i.next().unwrap();
        assert_eq!(second.0, &Condition::Signal(SIGUSR2));
        assert_eq!(second.1, None);
        assert_eq!(second.2.unwrap().action, command);
        assert_eq!(second.2.unwrap().origin, Some(origin_2));
        assert_eq!(i.next(), None);
    }

    #[test]
    fn iteration_after_setting_trap_in_subshell() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        let command = Action::Command("echo".into());
        trap_set
            .set_action(&mut system, SIGUSR1, command, origin_1, false)
            .unwrap();
        trap_set.enter_subshell(&mut system, false, false);
        let origin_2 = Location::dummy("bar");
        let command = Action::Command("ls".into());
        trap_set
            .set_action(
                &mut system,
                SIGUSR2,
                command.clone(),
                origin_2.clone(),
                false,
            )
            .unwrap();

        let mut i = trap_set.iter();
        let first = i.next().unwrap();
        assert_eq!(first.0, &Condition::Signal(SIGUSR2));
        assert_eq!(first.1.unwrap().action, command);
        assert_eq!(first.1.unwrap().origin, Some(origin_2));
        assert_eq!(first.2, None);
        assert_eq!(i.next(), None);
    }

    #[test]
    fn entering_subshell_resets_command_traps() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGCHLD, action.clone(), origin.clone(), false)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGCHLD),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGCHLD], Disposition::Default);
    }

    #[test]
    fn entering_subshell_keeps_ignore_traps() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGCHLD, Action::Ignore, origin.clone(), false)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGCHLD),
            (
                Some(&TrapState {
                    action: Action::Ignore,
                    origin: Some(origin),
                    pending: false
                }),
                None
            )
        );
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigchld() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGCHLD, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGCHLD),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigint() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGINT, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGINT),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGINT], Disposition::Default);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigterm() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGTERM, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGTERM),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGTERM], Disposition::Default);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigquit() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGQUIT, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGQUIT),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGQUIT], Disposition::Default);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigtstp() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGTSTP, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGTSTP),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGTSTP], Disposition::Default);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigttin() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGTTIN, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGTTIN),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGTTIN], Disposition::Default);
    }

    #[test]
    fn entering_subshell_with_internal_disposition_for_sigttou() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let action = Action::Command("".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGTTOU, action.clone(), origin.clone(), false)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();

        trap_set.enter_subshell(&mut system, false, false);
        assert_eq!(
            trap_set.get_state(SIGTTOU),
            (
                None,
                Some(&TrapState {
                    action,
                    origin: Some(origin),
                    pending: false
                })
            )
        );
        assert_eq!(system.0[&SIGTTOU], Disposition::Default);
    }

    #[test]
    fn setting_trap_after_entering_subshell_clears_parent_states() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        let command = Action::Command("echo 1".into());
        trap_set
            .set_action(&mut system, SIGUSR1, command, origin_1, false)
            .unwrap();
        let origin_2 = Location::dummy("bar");
        let command = Action::Command("echo 2".into());
        trap_set
            .set_action(&mut system, SIGUSR2, command, origin_2, false)
            .unwrap();
        trap_set.enter_subshell(&mut system, false, false);

        let command = Action::Command("echo 9".into());
        let origin_3 = Location::dummy("qux");
        trap_set
            .set_action(
                &mut system,
                SIGUSR1,
                command.clone(),
                origin_3.clone(),
                false,
            )
            .unwrap();

        assert_eq!(
            trap_set.get_state(SIGUSR1),
            (
                Some(&TrapState {
                    action: command,
                    origin: Some(origin_3),
                    pending: false
                }),
                None
            )
        );
        assert_eq!(trap_set.get_state(SIGUSR2), (None, None));
        assert_eq!(system.0[&SIGUSR1], Disposition::Catch);
        assert_eq!(system.0[&SIGUSR2], Disposition::Default);
    }

    #[test]
    fn entering_nested_subshell_clears_parent_states() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let origin_1 = Location::dummy("foo");
        let command = Action::Command("echo 1".into());
        trap_set
            .set_action(&mut system, SIGUSR1, command, origin_1, false)
            .unwrap();
        let origin_2 = Location::dummy("bar");
        let command = Action::Command("echo 2".into());
        trap_set
            .set_action(&mut system, SIGUSR2, command, origin_2, false)
            .unwrap();
        trap_set.enter_subshell(&mut system, false, false);
        trap_set.enter_subshell(&mut system, false, false);

        assert_eq!(trap_set.get_state(SIGUSR1), (None, None));
        assert_eq!(trap_set.get_state(SIGUSR2), (None, None));
        assert_eq!(system.0[&SIGUSR1], Disposition::Default);
        assert_eq!(system.0[&SIGUSR2], Disposition::Default);
    }

    #[test]
    fn ignoring_sigint_on_entering_subshell_with_action_set() {
        in_virtual_system(|mut env, state| async move {
            env.traps
                .set_action(
                    &mut env.system,
                    SIGINT,
                    Action::Command("".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            env.system.kill(env.main_pid, Some(SIGINT)).await.unwrap();
            env.traps.enter_subshell(&mut env.system, true, false);

            let state = state.borrow();
            let process = &state.processes[&env.main_pid];
            assert_eq!(process.disposition(SIGINT), Disposition::Ignore);
            assert_eq!(process.state(), ProcessState::Running);
        })
    }

    #[test]
    fn ignoring_sigquit_on_entering_subshell_with_action_set() {
        in_virtual_system(|mut env, state| async move {
            env.traps
                .set_action(
                    &mut env.system,
                    SIGQUIT,
                    Action::Command("".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            env.system.kill(env.main_pid, Some(SIGQUIT)).await.unwrap();
            env.traps.enter_subshell(&mut env.system, true, false);

            let state = state.borrow();
            let process = &state.processes[&env.main_pid];
            assert_eq!(process.disposition(SIGQUIT), Disposition::Ignore);
            assert_eq!(process.state(), ProcessState::Running);
        })
    }

    #[test]
    fn ignoring_sigint_and_sigquit_on_entering_subshell_without_action_set() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enter_subshell(&mut system, true, false);
        assert_eq!(system.0[&SIGINT], Disposition::Ignore);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
    }

    #[test]
    fn keeping_stopper_internal_dispositions_ignored() {
        in_virtual_system(|mut env, state| async move {
            for signal in [SIGTSTP, SIGTTIN, SIGTTOU] {
                env.traps
                    .set_action(
                        &mut env.system,
                        signal,
                        Action::Command("".into()),
                        Location::dummy(""),
                        false,
                    )
                    .unwrap();
            }
            env.traps
                .enable_internal_dispositions_for_stoppers(&mut env.system)
                .unwrap();
            for signal in [SIGTSTP, SIGTTIN, SIGTTOU] {
                env.system.kill(env.main_pid, Some(signal)).await.unwrap();
            }
            env.traps.enter_subshell(&mut env.system, false, true);

            let state = state.borrow();
            let process = &state.processes[&env.main_pid];
            assert_eq!(process.disposition(SIGTSTP), Disposition::Ignore);
            assert_eq!(process.disposition(SIGTTIN), Disposition::Ignore);
            assert_eq!(process.disposition(SIGTTOU), Disposition::Ignore);
            assert_eq!(process.state(), ProcessState::Running);
        })
    }

    #[test]
    fn no_stopper_internal_dispositions_enabled_to_be_kept_ignored() {
        in_virtual_system(|mut env, state| async move {
            for signal in [SIGTSTP, SIGTTIN, SIGTTOU] {
                env.traps
                    .set_action(
                        &mut env.system,
                        signal,
                        Action::Command("".into()),
                        Location::dummy(""),
                        false,
                    )
                    .unwrap();
            }
            env.traps.enter_subshell(&mut env.system, false, true);

            let state = state.borrow();
            let process = &state.processes[&env.main_pid];
            assert_eq!(process.disposition(SIGTSTP), Disposition::Default);
            assert_eq!(process.disposition(SIGTTIN), Disposition::Default);
            assert_eq!(process.disposition(SIGTTOU), Disposition::Default);
        })
    }

    #[test]
    fn catching_signal() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let command = Action::Command("echo INT".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGINT, command, origin, false)
            .unwrap();
        let command = Action::Command("echo TERM".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGTERM, command, origin, false)
            .unwrap();

        trap_set.catch_signal(SIGCHLD);
        trap_set.catch_signal(SIGINT);

        let trap_state = trap_set.get_state(SIGINT).0.unwrap();
        assert!(trap_state.pending, "trap_state = {trap_state:?}");
        let trap_state = trap_set.get_state(SIGTERM).0.unwrap();
        assert!(!trap_state.pending, "trap_state = {trap_state:?}");
    }

    #[test]
    fn taking_signal_if_caught() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        let command = Action::Command("echo INT".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGINT, command, origin, false)
            .unwrap();

        let result = trap_set.take_signal_if_caught(SIGINT);
        assert_eq!(result, None);

        trap_set.catch_signal(SIGINT);

        let result = trap_set.take_signal_if_caught(SIGINT);
        assert!(!result.unwrap().pending);

        let result = trap_set.take_signal_if_caught(SIGINT);
        assert_eq!(result, None);
    }

    #[test]
    fn taking_caught_signal() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        assert_eq!(trap_set.take_caught_signal(), None);

        let command = Action::Command("echo INT".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGINT, command, origin, false)
            .unwrap();
        let command = Action::Command("echo TERM".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGTERM, command, origin, false)
            .unwrap();
        let command = Action::Command("echo USR1".into());
        let origin = Location::dummy("origin");
        trap_set
            .set_action(&mut system, SIGUSR1, command, origin, false)
            .unwrap();
        assert_eq!(trap_set.take_caught_signal(), None);

        trap_set.catch_signal(SIGINT);
        trap_set.catch_signal(SIGUSR1);
        // The order in which take_caught_signal returns the two signals is
        // unspecified, so we accept both the orders.
        let result = trap_set.take_caught_signal().unwrap();
        match result.0 {
            SIGINT => {
                assert_eq!(result.1.action, Action::Command("echo INT".into()));
                assert!(!result.1.pending);

                let result = trap_set.take_caught_signal().unwrap();
                assert_eq!(result.0, SIGUSR1);
                assert_eq!(result.1.action, Action::Command("echo USR1".into()));
                assert!(!result.1.pending);
            }
            SIGUSR1 => {
                assert_eq!(result.1.action, Action::Command("echo USR1".into()));
                assert!(!result.1.pending);

                let result = trap_set.take_caught_signal().unwrap();
                assert_eq!(result.0, SIGINT);
                assert_eq!(result.1.action, Action::Command("echo INT".into()));
                assert!(!result.1.pending);
            }
            _ => panic!("wrong signal: {result:?}"),
        }

        assert_eq!(trap_set.take_caught_signal(), None);
    }

    #[test]
    fn enabling_internal_disposition_for_sigchld() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn enabling_internal_dispositions_for_terminators() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        assert_eq!(system.0[&SIGINT], Disposition::Catch);
        assert_eq!(system.0[&SIGTERM], Disposition::Ignore);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
    }

    #[test]
    fn enabling_internal_dispositions_for_stoppers() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        assert_eq!(system.0[&SIGTSTP], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTIN], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTOU], Disposition::Ignore);
    }

    #[test]
    fn disabling_internal_dispositions_for_initially_defaulted_signals() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        trap_set.disable_internal_dispositions(&mut system).unwrap();
        assert_eq!(system.0[&SIGCHLD], Disposition::Default);
        assert_eq!(system.0[&SIGINT], Disposition::Default);
        assert_eq!(system.0[&SIGTERM], Disposition::Default);
        assert_eq!(system.0[&SIGQUIT], Disposition::Default);
        assert_eq!(system.0[&SIGTSTP], Disposition::Default);
        assert_eq!(system.0[&SIGTTIN], Disposition::Default);
        assert_eq!(system.0[&SIGTTOU], Disposition::Default);
    }

    fn ignore_signals(system: &mut DummySystem) {
        system.0.extend(
            [SIGCHLD, SIGINT, SIGTERM, SIGQUIT, SIGTSTP, SIGTTIN, SIGTTOU]
                .into_iter()
                .map(|signal| (signal, Disposition::Ignore)),
        )
    }

    #[test]
    fn disabling_internal_dispositions_for_initially_ignored_signals() {
        let mut system = DummySystem::default();
        ignore_signals(&mut system);
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        trap_set.disable_internal_dispositions(&mut system).unwrap();
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
        assert_eq!(system.0[&SIGINT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTERM], Disposition::Ignore);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTSTP], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTIN], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTOU], Disposition::Ignore);
    }

    #[test]
    fn disabling_internal_dispositions_after_enabling_twice() {
        let mut system = DummySystem::default();
        ignore_signals(&mut system);
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        trap_set.disable_internal_dispositions(&mut system).unwrap();
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
        assert_eq!(system.0[&SIGINT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTERM], Disposition::Ignore);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTSTP], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTIN], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTOU], Disposition::Ignore);
    }

    #[test]
    fn disabling_internal_dispositions_without_enabling() {
        let mut system = DummySystem::default();
        ignore_signals(&mut system);
        let mut trap_set = TrapSet::default();
        trap_set.disable_internal_dispositions(&mut system).unwrap();
        assert_eq!(system.0[&SIGCHLD], Disposition::Ignore);
        assert_eq!(system.0[&SIGINT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTERM], Disposition::Ignore);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTSTP], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTIN], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTOU], Disposition::Ignore);
    }

    #[test]
    fn reenabling_internal_dispositions() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        trap_set.disable_internal_dispositions(&mut system).unwrap();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
        assert_eq!(system.0[&SIGINT], Disposition::Catch);
        assert_eq!(system.0[&SIGTERM], Disposition::Ignore);
        assert_eq!(system.0[&SIGQUIT], Disposition::Ignore);
        assert_eq!(system.0[&SIGTSTP], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTIN], Disposition::Ignore);
        assert_eq!(system.0[&SIGTTOU], Disposition::Ignore);
    }

    #[test]
    fn setting_trap_to_ignore_after_enabling_internal_disposition() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        let origin = Location::dummy("origin");
        let result = trap_set.set_action(&mut system, SIGCHLD, Action::Ignore, origin, false);
        assert_eq!(result, Ok(()));
        assert_eq!(system.0[&SIGCHLD], Disposition::Catch);
    }

    #[test]
    fn resetting_trap_from_ignore_no_override_after_enabling_internal_dispositions() {
        let mut system = DummySystem::default();
        ignore_signals(&mut system);
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();

        for signal in [SIGCHLD, SIGINT] {
            let origin = Location::dummy("origin");
            let result = trap_set.set_action(&mut system, signal, Action::Default, origin, false);
            assert_eq!(result, Err(SetActionError::InitiallyIgnored));
            assert_eq!(system.0[&signal], Disposition::Catch);
        }
        for signal in [SIGTERM, SIGQUIT, SIGTSTP, SIGTTIN, SIGTTOU] {
            let origin = Location::dummy("origin");
            let result = trap_set.set_action(&mut system, signal, Action::Default, origin, false);
            assert_eq!(result, Err(SetActionError::InitiallyIgnored));
            assert_eq!(system.0[&signal], Disposition::Ignore);
        }
    }

    #[test]
    fn resetting_trap_from_ignore_override_after_enabling_internal_dispositions() {
        let mut system = DummySystem::default();
        ignore_signals(&mut system);
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_stoppers(&mut system)
            .unwrap();

        for signal in [SIGCHLD, SIGINT] {
            let origin = Location::dummy("origin");
            let result =
                trap_set.set_action(&mut system, signal, Action::Ignore, origin.clone(), true);
            assert_eq!(result, Ok(()));
            assert_eq!(
                trap_set.get_state(signal),
                (
                    Some(&TrapState {
                        action: Action::Ignore,
                        origin: Some(origin),
                        pending: false
                    }),
                    None
                )
            );
            assert_eq!(system.0[&signal], Disposition::Catch);
        }
        for signal in [SIGTERM, SIGQUIT, SIGTSTP, SIGTTIN, SIGTTOU] {
            let origin = Location::dummy("origin");
            let result =
                trap_set.set_action(&mut system, signal, Action::Ignore, origin.clone(), true);
            assert_eq!(result, Ok(()));
            assert_eq!(
                trap_set.get_state(signal),
                (
                    Some(&TrapState {
                        action: Action::Ignore,
                        origin: Some(origin),
                        pending: false
                    }),
                    None
                )
            );
            assert_eq!(system.0[&signal], Disposition::Ignore);
        }
    }

    #[test]
    fn disabling_internal_disposition_with_ignore_trap() {
        let signals = [SIGCHLD, SIGINT, SIGTERM, SIGQUIT, SIGTSTP, SIGTTIN, SIGTTOU];

        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set
            .enable_internal_disposition_for_sigchld(&mut system)
            .unwrap();
        trap_set
            .enable_internal_dispositions_for_terminators(&mut system)
            .unwrap();
        let origin = Location::dummy("origin");
        for signal in signals {
            trap_set
                .set_action(&mut system, signal, Action::Ignore, origin.clone(), false)
                .unwrap();
        }
        trap_set.disable_internal_dispositions(&mut system).unwrap();

        for signal in signals {
            assert_eq!(
                trap_set.get_state(signal),
                (
                    Some(&TrapState {
                        action: Action::Ignore,
                        origin: Some(origin.clone()),
                        pending: false
                    }),
                    None
                )
            );
            assert_eq!(system.0[&signal], Disposition::Ignore);
        }
    }
}
