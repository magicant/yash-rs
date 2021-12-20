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
use std::iter::Peekable;
pub use yash_env::semantics::Field;

/// Specification for an options's argument
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OptionArgumentSpec {
    /// The option does not take an argument. (default)
    None,
    /// The option requires an argument.
    Required,
    // /// The option may have an argument.
    // Optional,
}

impl Default for OptionArgumentSpec {
    fn default() -> Self {
        OptionArgumentSpec::None
    }
}

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
    argument: OptionArgumentSpec,
}

impl OptionSpec {
    /// Creates a new empty option spec.
    pub const fn new() -> Self {
        OptionSpec {
            short: None,
            argument: OptionArgumentSpec::None,
        }
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

    /// Returns whether this option takes an argument.
    pub const fn get_argument(&self) -> OptionArgumentSpec {
        self.argument
    }

    /// Specifies whether this option takes an argument.
    pub fn set_argument(&mut self, argument: OptionArgumentSpec) {
        self.argument = argument;
    }

    /// Chained version of [`set_argument`](Self::set_argument)
    pub const fn argument(mut self, argument: OptionArgumentSpec) -> Self {
        self.argument = argument;
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

    /// Argument to this option.
    ///
    /// This value is always `None` for an option that does not take an argument.
    ///
    /// This value always contains a field for an option that requires an argument.
    ///
    /// If the option name and its argument are given in a single field,
    /// the field value is modified to contain only the option argument.
    pub argument: Option<Field>,
}

/// TODO
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {}

/// TODO impl std::error::Error for Error

/// Parses short options in an argument.
///
/// This function examines the first field yielded by `arguments` and consumes
/// it if it contains one or more short options. If the last option requires an
/// argument and the field does not include one, the following field is consumed
/// as the argument.
///
/// This function returns `true` if consumed one or more fields.
fn parse_short_options<'a, I: Iterator<Item = Field>>(
    option_specs: &'a [OptionSpec],
    arguments: &mut Peekable<I>,
    option_occurrences: &mut Vec<OptionOccurrence<'a>>,
) -> bool {
    let argument = match arguments.peek() {
        Some(argument) => argument,
        None => return false,
    };

    let mut chars = argument.value.chars();
    if chars.next() != Some('-') {
        // argument.value not starting with a hyphen
        return false;
    }
    if chars.as_str().is_empty() {
        // argument.value == "-"
        return false;
    }
    if chars.as_str() == "-" {
        // argument.value == "--"
        return false;
    }

    while let Some(c) = chars.next() {
        if let Some(spec) = option_specs.iter().find(|spec| spec.get_short() == Some(c)) {
            match spec.get_argument() {
                OptionArgumentSpec::None => {
                    let argument = None;
                    option_occurrences.push(OptionOccurrence { spec, argument });
                }
                OptionArgumentSpec::Required => {
                    let remainder_len = chars.as_str().len();
                    let argument = if remainder_len == 0 {
                        // The option argument is the next command-line argument.
                        drop(arguments.next());
                        arguments.next()
                        // TODO Error if argument is none
                    } else {
                        // The option argument is the rest of the current command-line argument.
                        let prefix = argument.value.len() - remainder_len;
                        let mut argument = arguments.next().unwrap();
                        argument.value.drain(..prefix);
                        Some(argument)
                    };
                    option_occurrences.push(OptionOccurrence { spec, argument });
                    return true;
                }
            };
        }
    }
    drop(arguments.next());
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
    arguments: Vec<Field>,
) -> Result<(Vec<OptionOccurrence<'_>>, Vec<Field>), Error> {
    let mut arguments = arguments.into_iter().skip(1).peekable();

    let mut option_occurrences = vec![];
    while parse_short_options(option_specs, &mut arguments, &mut option_occurrences) {}

    arguments.next_if(|argument| argument.value == "--");

    let operands = arguments.collect();
    Ok((option_occurrences, operands))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

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

    #[test]
    fn single_hyphen_argument_is_not_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["foo", "-"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["-"]));

        let arguments = Field::dummies(["foo", "-", "-"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["-", "-"]));
    }

    #[test]
    fn options_are_not_recognized_after_separator() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["foo", "-a", "--", "-a", "--", "-a"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(operands, Field::dummies(["-a", "--", "-a"]));
    }

    #[test]
    fn options_are_not_recognized_after_operand_by_default() {
        let specs = &[OptionSpec::new().short('x'), OptionSpec::new().short('y')];

        let arguments = Field::dummies(["foo", "-x", "bar", "-y", "baz"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('x'));
        assert_eq!(operands, Field::dummies(["bar", "-y", "baz"]));
    }

    #[test]
    fn adjacent_argument_to_short_option() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["foo", "-abar"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "bar");
            assert_eq!(field.origin.line.value, "-abar");
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", "-a1", "-a2", "3"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "1");
            assert_eq!(field.origin.line.value, "-a1");
        });
        assert_eq!(options[1].spec.get_short(), Some('a'));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "2");
            assert_eq!(field.origin.line.value, "-a2");
        });
        assert_eq!(operands, Field::dummies(["3"]));
    }

    #[test]
    fn separate_argument_to_short_option() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["foo", "-a", "bar"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "bar");
            assert_eq!(field.origin.line.value, "bar");
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", "-a", "1", "-a", "2", "3"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "1");
            assert_eq!(field.origin.line.value, "1");
        });
        assert_eq!(options[1].spec.get_short(), Some('a'));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "2");
            assert_eq!(field.origin.line.value, "2");
        });
        assert_eq!(operands, Field::dummies(["3"]));
    }

    #[test]
    fn argument_taking_option_adjacent_to_another_option() {
        let specs = &[
            OptionSpec::new()
                .short('a')
                .argument(OptionArgumentSpec::None),
            OptionSpec::new()
                .short('b')
                .argument(OptionArgumentSpec::None),
            OptionSpec::new()
                .short('c')
                .argument(OptionArgumentSpec::Required),
        ];

        let arguments = Field::dummies(["foo", "-abcdef"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(options[0].argument, None);
        assert_eq!(options[1].spec.get_short(), Some('b'));
        assert_eq!(options[1].argument, None);
        assert_eq!(options[2].spec.get_short(), Some('c'));
        assert_matches!(options[2].argument, Some(ref field) => {
            assert_eq!(field.value, "def");
            assert_eq!(field.origin.line.value, "-abcdef");
        });
        assert_eq!(operands, []);
    }

    #[test]
    fn empty_argument_to_short_option() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["foo", "-a", ""]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "");
            assert_eq!(field.origin.line.value, "");
        });
        assert_eq!(operands, []);
    }

    #[test]
    fn option_argument_that_looks_like_separator() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["foo", "-a", "argument", "-a", "--", "--", "operand"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "argument");
            assert_eq!(field.origin.line.value, "argument");
        });
        assert_eq!(options[1].spec.get_short(), Some('a'));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "--");
            assert_eq!(field.origin.line.value, "--");
        });
        assert_eq!(operands, Field::dummies(["operand"]));
    }

    // TODO options_are_recognized_after_operand (depending mode)

    // TODO missing_argument_to_short_option
}
