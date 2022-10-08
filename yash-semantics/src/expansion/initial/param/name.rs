// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Parameter name

use std::num::IntErrorKind::PosOverflow;
use yash_syntax::parser::lex::is_name_char;
use yash_syntax::parser::lex::is_special_parameter_char;

/// Parameter name
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Name<'a> {
    /// Variable
    ///
    /// The name must be a non-empty alphanumeric string that may contain `_`
    /// but cannot start with a digit.
    Variable(&'a str),

    /// Special parameter
    ///
    /// The character must be one of `@*#?-$!0`.
    Special(char),

    /// Positional parameter
    ///
    /// Note that positional parameters count starting from 1.
    /// `Positional(0)`, which results from `"00"`, etc., should be regarded as
    /// a non-existing parameter.
    ///
    /// Positional parameters with a too large index are represented by
    /// `Positional(usize::MAX)`, which should also regarded as non-existing.
    Positional(usize),
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct InvalidName;

impl<'a> TryFrom<&'a str> for Name<'a> {
    type Error = InvalidName;

    fn try_from(s: &'a str) -> Result<Self, InvalidName> {
        let mut cs = s.chars();
        let first = cs.next().ok_or(InvalidName)?;
        if cs.next() == None && is_special_parameter_char(first) {
            return Ok(Name::Special(first));
        }

        if first.is_ascii_digit() {
            match s.parse() {
                Ok(index) => Ok(Name::Positional(index)),
                Err(e) => {
                    if e.kind() == &PosOverflow {
                        Ok(Name::Positional(usize::MAX))
                    } else {
                        Err(InvalidName)
                    }
                }
            }
        } else if s.chars().all(is_name_char) {
            Ok(Name::Variable(s))
        } else {
            Err(InvalidName)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_try_from_variable() {
        let name = "foo".try_into();
        assert_eq!(name, Ok(Name::Variable("foo")));
        let name = "bar".try_into();
        assert_eq!(name, Ok(Name::Variable("bar")));
        let name = "_09".try_into();
        assert_eq!(name, Ok(Name::Variable("_09")));
        let name = "a_B_0".try_into();
        assert_eq!(name, Ok(Name::Variable("a_B_0")));
    }

    #[test]
    fn name_try_from_special() {
        let name = "@".try_into();
        assert_eq!(name, Ok(Name::Special('@')));
        let name = "*".try_into();
        assert_eq!(name, Ok(Name::Special('*')));
        let name = "#".try_into();
        assert_eq!(name, Ok(Name::Special('#')));
        let name = "?".try_into();
        assert_eq!(name, Ok(Name::Special('?')));
        let name = "-".try_into();
        assert_eq!(name, Ok(Name::Special('-')));
        let name = "$".try_into();
        assert_eq!(name, Ok(Name::Special('$')));
        let name = "!".try_into();
        assert_eq!(name, Ok(Name::Special('!')));
        let name = "0".try_into();
        assert_eq!(name, Ok(Name::Special('0')));
    }

    #[test]
    fn name_try_from_positional() {
        let name = "1".try_into();
        assert_eq!(name, Ok(Name::Positional(1)));
        let name = "2".try_into();
        assert_eq!(name, Ok(Name::Positional(2)));
        let name = "11".try_into();
        assert_eq!(name, Ok(Name::Positional(11)));
        let name = "346".try_into();
        assert_eq!(name, Ok(Name::Positional(346)));
        let name = "0000".try_into();
        assert_eq!(name, Ok(Name::Positional(0)));
        let name = "99999999999999999999".try_into();
        assert_eq!(name, Ok(Name::Positional(usize::MAX)));
    }

    #[test]
    fn invalid_name() {
        let name = TryInto::<Name>::try_into("");
        assert_eq!(name, Err(InvalidName));
        let name = TryInto::<Name>::try_into(">");
        assert_eq!(name, Err(InvalidName));
        let name = TryInto::<Name>::try_into("%");
        assert_eq!(name, Err(InvalidName));
        let name = TryInto::<Name>::try_into("0a");
        assert_eq!(name, Err(InvalidName));
        let name = TryInto::<Name>::try_into("0_0");
        assert_eq!(name, Err(InvalidName));
        let name = TryInto::<Name>::try_into("2x");
        assert_eq!(name, Err(InvalidName));
        let name = TryInto::<Name>::try_into("abc-def");
        assert_eq!(name, Err(InvalidName));
    }
}
