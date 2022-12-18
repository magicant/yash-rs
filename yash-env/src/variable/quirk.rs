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

//! Quirks of variables

use super::Value;
use super::Variable;
use std::borrow::Cow;
use yash_syntax::source::Location;
use yash_syntax::source::Source;

/// Special characteristics of a variable
///
/// While most variables act as a simple store of a value, some variables
/// exhibit special effects when they get expanded or assigned to. Such
/// variables may have their value computed dynamically on expansion or may have
/// an internal state that is updated when the value is set. `Quirk` determines
/// the nature of a variable and contains the relevant state.
///
/// Use [`Variable::expand`] to apply the variable's quirk when expanding a
/// variable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Quirk {
    /// Quirk for the `$LINENO` variable
    ///
    /// The value of a variable having this variant of `Quirk` is computed
    /// dynamically from the expanding context. The result is the line number of
    /// the location of the parameter expansion. This `Quirk` is lost when an
    /// assignment sets a new value to the variable.
    LineNumber,
    // TODO $RANDOM
    // TODO $PATH
}

/// Expanded value of a variable
///
/// Variables with a [`Quirk`] may have their values computed dynamically when
/// expanded, hence [`Cow`] in the enum values.
/// Use [`Variable::expand`] to get an `Expansion` instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expansion<'a> {
    /// The value does not exist.
    Unset,
    /// The value is a single string.
    Scalar(Cow<'a, str>),
    /// The value is an array of strings.
    Array(Cow<'a, [String]>),
}

impl From<String> for Expansion<'static> {
    fn from(value: String) -> Self {
        Expansion::Scalar(Cow::Owned(value))
    }
}

impl<'a> From<&'a str> for Expansion<'a> {
    fn from(value: &'a str) -> Self {
        Expansion::Scalar(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for Expansion<'a> {
    fn from(value: &'a String) -> Self {
        Expansion::Scalar(Cow::Borrowed(value))
    }
}

impl From<Option<String>> for Expansion<'static> {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Expansion::Unset,
        }
    }
}

impl From<Vec<String>> for Expansion<'static> {
    fn from(values: Vec<String>) -> Self {
        Expansion::Array(Cow::Owned(values))
    }
}

impl<'a> From<&'a [String]> for Expansion<'a> {
    fn from(values: &'a [String]) -> Self {
        Expansion::Array(Cow::Borrowed(values))
    }
}

impl<'a> From<&'a Vec<String>> for Expansion<'a> {
    fn from(values: &'a Vec<String>) -> Self {
        Expansion::Array(Cow::Borrowed(values))
    }
}

impl From<Value> for Expansion<'static> {
    fn from(value: Value) -> Self {
        match value {
            Value::Scalar(value) => Expansion::from(value),
            Value::Array(values) => Expansion::from(values),
        }
    }
}

impl<'a> From<&'a Value> for Expansion<'a> {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Scalar(value) => Expansion::from(value),
            Value::Array(values) => Expansion::from(values),
        }
    }
}

impl From<Option<Value>> for Expansion<'static> {
    fn from(value: Option<Value>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Expansion::Unset,
        }
    }
}

impl<'a, V> From<Option<&'a V>> for Expansion<'a>
where
    Expansion<'a>: From<&'a V>,
{
    fn from(value: Option<&'a V>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Expansion::Unset,
        }
    }
}

impl Expansion<'_> {
    /// Converts into an owned value
    #[must_use]
    pub fn into_owned(self) -> Option<Value> {
        match self {
            Expansion::Unset => None,
            Expansion::Scalar(value) => Some(Value::Scalar(value.into_owned())),
            Expansion::Array(values) => Some(Value::Array(values.into_owned())),
        }
    }

    /// Returns the "length" of the value.
    ///
    /// For `Unset`, the length is 0.
    /// For `Scalar`, the length is the number of characters.
    /// For `Array`, the length is the number of strings.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Expansion::Unset => 0,
            Expansion::Scalar(value) => value.len(),
            Expansion::Array(values) => values.len(),
        }
    }

    /// Tests whether the [length](Self::len) is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Variable {
    // TODO Should require mutable self
    /// Returns the value of this variable, applying any quirk.
    ///
    /// If this variable has no [`Quirk`], this function just returns
    /// `self.value` converted to [`Expansion`]. Otherwise, the effect of the
    /// quirk is applied to the value and the result is returned.
    ///
    /// This function requires the location of the parameter expanding this
    /// variable, so that `Quirk::LineNumber` can yield the line number of the
    /// location.
    pub fn expand(&self, mut location: &Location) -> Expansion {
        match &self.quirk {
            None => self.value.as_ref().into(),

            Some(Quirk::LineNumber) => {
                while let Source::Alias { original, .. } = &location.code.source {
                    location = original;
                }
                let count = location
                    .code
                    .value
                    .borrow()
                    .chars()
                    .take(location.range.start)
                    .filter(|c| *c == '\n')
                    .count()
                    .try_into()
                    .unwrap_or(u64::MAX);
                let line_number = u64::from(location.code.start_line_number).saturating_add(count);
                line_number.to_string().into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use std::rc::Rc;
    use yash_syntax::alias::Alias;
    use yash_syntax::source::Code;

    #[test]
    fn expand_no_quirk() {
        let var = Variable::new("foo");
        let loc = Location::dummy("somewhere");
        let result = var.expand(&loc);
        assert_eq!(result, Expansion::Scalar("foo".into()));
    }

    fn stub_code() -> Rc<Code> {
        Code {
            value: "foo\nbar\nbaz\n".to_string().into(),
            start_line_number: NonZeroU64::new(42).unwrap(),
            source: Source::Unknown,
        }
        .into()
    }

    #[test]
    fn expand_line_number_of_first_line() {
        let var = Variable {
            quirk: Some(Quirk::LineNumber),
            ..Default::default()
        };
        let code = stub_code();
        let range = 1..3;
        let loc = Location { code, range };
        let result = var.expand(&loc);
        assert_eq!(result, Expansion::Scalar("42".into()));
    }

    #[test]
    fn expand_line_number_of_third_line() {
        let var = Variable {
            quirk: Some(Quirk::LineNumber),
            ..Default::default()
        };
        let code = stub_code();
        let range = 8..12;
        let loc = Location { code, range };
        let result = var.expand(&loc);
        assert_eq!(result, Expansion::Scalar("44".into()));
    }

    #[test]
    fn expand_line_number_in_alias() {
        fn to_alias(original: Location) -> Location {
            let alias = Alias {
                name: "name".to_string(),
                replacement: "replacement".to_string(),
                global: false,
                origin: Location::dummy("alias"),
            }
            .into();
            let code = Code {
                value: " \n \n ".to_string().into(),
                start_line_number: NonZeroU64::new(15).unwrap(),
                source: Source::Alias { original, alias },
            }
            .into();
            let range = 0..1;
            Location { code, range }
        }

        let var = Variable {
            quirk: Some(Quirk::LineNumber),
            ..Default::default()
        };
        let code = stub_code();
        let range = 8..12;
        let loc = to_alias(to_alias(Location { code, range }));
        let result = var.expand(&loc);
        assert_eq!(result, Expansion::Scalar("44".into()));
    }
}
