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

//! State verification for the getopts built-in
//!
//! The getopts built-in is designed to be called multiple times with the same
//! arguments, assuming that the `$OPTIND` variable isn't altered externally.
//! This module provides utility to verify if the built-in receives the same
//! arguments and `$OPTIND` value in subsequent calls.

use thiserror::Error;

/// Type of error returned by [`GetoptsStateRef::verify`]
#[derive(Clone, Copy, Debug, Eq, Error, Hash, PartialEq)]
pub enum Error {
    /// The built-in receives different arguments than the previous call.
    #[error("arguments are different from the previous call")]
    DifferentArgs,
    /// The `$OPTIND` variable is altered externally.
    #[error("$OPTIND has been modified externally")]
    DifferentOptind,
}

/// Origin of the arguments parsed by the getopts built-in
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Origin {
    /// The arguments are passed directly to the built-in.
    DirectArgs,
    /// No arguments are passed to the built-in, so the built-in parses the
    /// positional parameters.
    PositionalParams,
}

/// State shared between getopts built-in invocations
///
/// The getopts built-in is designed to be called multiple times with the same
/// arguments, assuming that the `$OPTIND` variable isn't altered externally.
/// The built-in stores the arguments and the `$OPTIND` value in this data, and
/// verifies if it receives the same arguments and `$OPTIND` value in subsequent
/// calls.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GetoptsState {
    /// Expected arguments to parse
    pub args: Vec<String>,
    /// Expected origin of the arguments
    pub origin: Origin,
    /// Expected value of `$OPTIND`
    pub optind: String,
}

/// Borrowed version of [`GetoptsState`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GetoptsStateRef<'a, I> {
    pub args: I,
    pub origin: Origin,
    pub optind: &'a str,
}

impl<'a> From<&'a GetoptsState> for GetoptsStateRef<'a, std::slice::Iter<'a, String>> {
    fn from(state: &'a GetoptsState) -> Self {
        GetoptsStateRef {
            args: state.args.iter(),
            origin: state.origin,
            optind: &state.optind,
        }
    }
}

impl<I> GetoptsStateRef<'_, I> {
    /// Clones the referenced data into [`GetoptsState`].
    #[must_use]
    pub fn into_state(self) -> GetoptsState
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        GetoptsState {
            args: self.args.into_iter().map(Into::into).collect(),
            origin: self.origin,
            optind: self.optind.into(),
        }
    }

    /// Verifies if the given state is the same as `self`.
    ///
    /// This function returns an error if the given state is different from
    /// `self`. Exceptionally, if `self.optind` is `1`, this function ignores
    /// the given state and returns `Ok(Some(self))`. Otherwise, this function
    /// returns `Ok(None)`.
    pub fn verify<'a, S, J>(self, previous: S) -> Result<Option<Self>, Error>
    where
        I: IntoIterator,
        S: Into<GetoptsStateRef<'a, J>>,
        J: IntoIterator,
        I::Item: PartialEq<J::Item>,
    {
        if self.optind == "1" {
            return Ok(Some(self));
        }

        let previous = previous.into();

        if self.origin != previous.origin {
            return Err(Error::DifferentArgs);
        }

        if self.args.into_iter().ne(previous.args) {
            return Err(Error::DifferentArgs);
        }

        if self.optind != previous.optind {
            return Err(Error::DifferentOptind);
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn verify_with_optind_1() {
        let left = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::DirectArgs,
            optind: "1".into(),
        };
        let right = GetoptsState {
            args: vec!["-x".into(), "-y".into()],
            origin: Origin::PositionalParams,
            optind: "2".into(),
        };

        let result = GetoptsStateRef::from(&left).verify(&right);
        assert_matches!(result, Ok(Some(state_ref)) => {
            assert_eq!(state_ref.into_state(), left);
        });
    }

    #[test]
    fn verify_with_same_states() {
        let state = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::DirectArgs,
            optind: "2".into(),
        };

        let result = GetoptsStateRef::from(&state).verify(&state);
        assert_matches!(result, Ok(None));
    }

    #[test]
    fn verify_with_different_args() {
        let left = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::DirectArgs,
            optind: "2".into(),
        };
        let right = GetoptsState {
            args: vec!["-a".into(), "-c".into()],
            origin: Origin::DirectArgs,
            optind: "2".into(),
        };

        let result = GetoptsStateRef::from(&left).verify(&right);
        assert_matches!(result, Err(Error::DifferentArgs));
    }

    #[test]
    fn verify_with_different_origins() {
        let left = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::DirectArgs,
            optind: "2".into(),
        };
        let right = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::PositionalParams,
            optind: "2".into(),
        };

        let result = GetoptsStateRef::from(&left).verify(&right);
        assert_matches!(result, Err(Error::DifferentArgs));
    }

    #[test]
    fn verify_with_different_optind_values() {
        let left = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::DirectArgs,
            optind: "2".into(),
        };
        let right = GetoptsState {
            args: vec!["-a".into(), "-b".into()],
            origin: Origin::DirectArgs,
            optind: "3".into(),
        };

        let result = GetoptsStateRef::from(&left).verify(&right);
        assert_matches!(result, Err(Error::DifferentOptind));
    }
}
