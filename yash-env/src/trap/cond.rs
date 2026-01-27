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

//! Items that define trap conditions

#[cfg(doc)]
use super::state::Action;
use crate::signal;
use crate::system::Signals;
use itertools::Itertools as _;
use std::borrow::Cow;

/// Condition under which an [`Action`] is executed
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Condition {
    /// When the shell exits
    Exit,
    /// When the specified signal is delivered to the shell process
    Signal(signal::Number),
}

impl From<signal::Number> for Condition {
    fn from(number: signal::Number) -> Self {
        Self::Signal(number)
    }
}

/// Conversion from raw signal number to `Condition`
///
/// If the number is zero, the result is [`Condition::Exit`]. Otherwise, the
/// result is [`Condition::Signal`] with the signal number.
impl From<signal::RawNumber> for Condition {
    fn from(number: signal::RawNumber) -> Self {
        if let Ok(non_zero) = number.try_into() {
            Self::Signal(signal::Number::from_raw_unchecked(non_zero))
        } else {
            Self::Exit
        }
    }
}

impl From<Condition> for signal::RawNumber {
    fn from(cond: Condition) -> Self {
        match cond {
            Condition::Exit => 0,
            Condition::Signal(number) => number.as_raw(),
        }
    }
}

impl Condition {
    /// Converts this `Condition` to a `String`.
    ///
    /// The result is an uppercase string representing the condition such as
    /// `"EXIT"` and `"TERM"`. Signal names are obtained from
    /// [`Signals::sig2str`].
    #[must_use]
    pub fn to_string<S: Signals>(&self, system: &S) -> Cow<'static, str> {
        match self {
            Self::Exit => Cow::Borrowed("EXIT"),
            Self::Signal(number) => system.sig2str(*number).unwrap_or(Cow::Borrowed("?")),
        }
    }

    /// Returns an iterator over all possible conditions.
    ///
    /// The iterator yields all the conditions supported by the given `Signals`
    /// implementation.
    /// The iteration starts with [`Condition::Exit`], followed by all the
    /// signals in the ascending order of their signal numbers.
    // TODO Most part of this function is duplicated from yash_builtin::kill::print::all_signals.
    // Consider refactoring to share the code. Note that all_signals does not
    // deduplicate the signals.
    pub fn iter<S: Signals>(system: &S) -> impl Iterator<Item = Condition> + '_ {
        let non_real_time = S::NAMED_SIGNALS
            .iter()
            .filter_map(|&(_, number)| Some(Condition::Signal(number?)));
        let non_real_time_count = S::NAMED_SIGNALS.len();

        let real_time = system.iter_sigrt().map(Condition::Signal);
        let real_time_count = real_time.size_hint().1.unwrap_or_default();

        let mut conditions = Vec::with_capacity(1 + non_real_time_count + real_time_count);
        conditions.push(Condition::Exit);
        conditions.extend(non_real_time);
        conditions.extend(real_time);
        conditions.sort();
        // Some names may share the same number, so deduplicate.
        conditions.into_iter().dedup()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::VirtualSystem;

    #[test]
    fn condition_iter_is_sorted() {
        let system = VirtualSystem::new();
        let iter = Condition::iter(&system);
        assert!(iter.is_sorted());
    }

    #[test]
    fn condition_iter_is_unique() {
        let system = VirtualSystem::new();
        let iter = Condition::iter(&system);
        let iter_dedup = Condition::iter(&system).dedup();
        assert!(iter.eq(iter_dedup));
    }
}
