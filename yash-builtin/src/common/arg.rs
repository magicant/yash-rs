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

//! Command-line argument parser
//!
//! This module provides functionalities for parsing command-line arguments into
//! options and operands.
//!
//! This module's parser can parse command lines that adhere to POSIX Utility
//! Syntax Guidelines and support non-standard syntax extensions such as long
//! options and options after operands.
//!
//! # Example
//!
//! TODO

#[doc(no_inline)]
pub use yash_env::semantics::Field;

/// Specification of an option
///
/// This structure may contain the following properties:
///
/// - Short option name (a single character)
/// - Long option name (a string)
/// - Whether this option takes an argument
///
/// All of these are optional, but either or both of the short and long names
/// should be set for the option spec to have meaningful effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionSpec {
    short: Option<char>,
}

impl OptionSpec {
    /// Creates a new empty option spec.
    pub const fn new() -> Self {
        OptionSpec { short: None }
    }

    /// Returns the short option name.
    pub const fn get_short(&self) -> Option<char> {
        self.short
    }

    /// Gives a short name for this option.
    pub fn set_short(&mut self, name: char) {
        self.short = Some(name);
    }

    /// Chained version of [`set_short`](Self::set_short)
    pub const fn short(mut self, name: char) -> Self {
        self.short = Some(name);
        self
    }
}

/// TODO
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct Mode {}

/// Occurrence of an option
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionOccurrence<'a> {
    /// Specification for this option.
    pub spec: &'a OptionSpec,
}

/// TODO
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {}

/// TODO impl std::error::Error for Error

/// Parses short options in an argument.
///
/// Returns true iff one or more options were found.
fn parse_short_options<'a>(
    option_specs: &'a [OptionSpec],
    argument: &Field,
    option_occurrences: &mut Vec<OptionOccurrence<'a>>,
) -> bool {
    let mut i = argument.value.chars();
    if i.next() != Some('-') {
        return false;
    }

    for c in i {
        if let Some(spec) = option_specs.iter().find(|spec| spec.get_short() == Some(c)) {
            option_occurrences.push(OptionOccurrence { spec });
        }
    }
    // TODO Return false if no options were found
    true
}

/// Parses command-line arguments into options and operands.
///
/// The first argument is always dropped and the remaining arguments are parsed.
///
/// If successful, returns a pair of option occurrences and operands.
pub fn parse_arguments(
    option_specs: &[OptionSpec],
    _mode: Mode,
    mut arguments: Vec<Field>,
) -> Result<(Vec<OptionOccurrence<'_>>, Vec<Field>), Error> {
    if !arguments.is_empty() {
        arguments.remove(0);
    }
    if let Some(argument) = arguments.first() {
        if argument.value == "--" {
            arguments.remove(0);
        }
    }

    let mut option_occurrences = vec![];
    while let Some(argument) = arguments.first() {
        if parse_short_options(option_specs, argument, &mut option_occurrences) {
            arguments.remove(0);
        } else {
            break;
        }
    }
    Ok((option_occurrences, arguments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_arguments() {
        let (options, operands) = parse_arguments(&[], Mode::default(), vec![]).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);
    }

    #[test]
    fn only_operands() {
        let arguments = Field::dummies(["foo"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", ""]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies([""]));

        let arguments = Field::dummies(["foo", "bar", "", "baz"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["bar", "", "baz"]));
    }

    #[test]
    fn operands_following_separator() {
        let arguments = Field::dummies(["command", "--"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);

        let arguments = Field::dummies(["command", "--", "1"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["1"]));

        let arguments = Field::dummies(["command", "--", "a", "", "z"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["a", "", "z"]));
    }

    #[test]
    fn non_occurring_short_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["foo"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", ""]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies([""]));
    }

    #[test]
    fn single_occurrence_of_short_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["foo", "-a"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", "-a", "bar"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(operands, Field::dummies(["bar"]));
    }

    #[test]
    fn multiple_occurrences_of_same_option_spec() {
        let specs = &[OptionSpec::new().short('b')];

        let arguments = Field::dummies(["command", "-b", "-b"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('b'));
        assert_eq!(options[1].spec.get_short(), Some('b'));
        assert_eq!(operands, []);

        let arguments = Field::dummies(["command", "-b", "-b", "argument"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('b'));
        assert_eq!(options[1].spec.get_short(), Some('b'));
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn occurrences_of_multiple_option_specs() {
        let specs = &[OptionSpec::new().short('x'), OptionSpec::new().short('y')];

        let arguments = Field::dummies(["command", "-x", "-y", "!"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('x'));
        assert_eq!(options[1].spec.get_short(), Some('y'));
        assert_eq!(operands, Field::dummies(["!"]));

        let arguments = Field::dummies(["command", "-y", "-x", "-y"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('y'));
        assert_eq!(options[1].spec.get_short(), Some('x'));
        assert_eq!(options[2].spec.get_short(), Some('y'));
        assert_eq!(operands, []);
    }

    #[test]
    fn multiple_occurrences_of_short_options_in_single_argument() {
        let specs = &[OptionSpec::new().short('p'), OptionSpec::new().short('q')];

        let arguments = Field::dummies(["command", "-pq", "!"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('p'));
        assert_eq!(options[1].spec.get_short(), Some('q'));
        assert_eq!(operands, Field::dummies(["!"]));

        let arguments = Field::dummies(["command", "-qpq"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('q'));
        assert_eq!(options[1].spec.get_short(), Some('p'));
        assert_eq!(options[2].spec.get_short(), Some('q'));
        assert_eq!(operands, []);
    }

    // TODO single_hyphen_argument_is_not_option
    // TODO options_are_not_recognized_after_separator
}
