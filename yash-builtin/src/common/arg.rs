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
pub struct OptionSpec<'a> {
    short: Option<char>,
    long: Option<&'a str>,
    argument: OptionArgumentSpec,
}

impl OptionSpec<'static> {
    /// Creates a new empty option spec.
    pub const fn new() -> Self {
        OptionSpec {
            short: None,
            long: None,
            argument: OptionArgumentSpec::None,
        }
    }
}

impl OptionSpec<'_> {
    /// Returns the short option name.
    pub const fn get_short(&self) -> Option<char> {
        self.short
    }

    /// Gives a short name for this option.
    ///
    /// The name should not be a hyphen.
    pub fn set_short(&mut self, name: char) {
        self.short = Some(name);
    }

    /// Chained version of [`set_short`](Self::set_short)
    pub const fn short(mut self, name: char) -> Self {
        self.short = Some(name);
        self
    }
}

impl<'a> OptionSpec<'a> {
    /// Returns the long option name.
    pub const fn get_long(&self) -> Option<&'a str> {
        self.long
    }

    /// Gives a long name for this option.
    ///
    /// The name should not start with `"--"` or include `"="`.
    pub fn set_long(&mut self, name: &'a str) {
        self.long = Some(name);
    }

    /// Chained version of [`set_long`](Self::set_long)
    pub const fn long(mut self, name: &'a str) -> Self {
        self.long = Some(name);
        self
    }
}

impl OptionSpec<'_> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LongMatch {
    None,
    Partial,
    Exact,
}

impl OptionSpec<'_> {
    fn long_match(&self, name: &str) -> LongMatch {
        if let Some(long) = self.long {
            if long.starts_with(name) {
                return if long.len() == name.len() {
                    LongMatch::Exact
                } else {
                    LongMatch::Partial
                };
            }
        }
        LongMatch::None
    }
}

/// TODO
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct Mode {}

/// Occurrence of an option
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionOccurrence<'a> {
    /// Specification for this option.
    pub spec: &'a OptionSpec<'a>,

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

/// Error in command line parsing
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error<'a> {
    /// Short option that is not defined in the option specs
    UnknownShortOption(char, Field),

    /// Long option that is not defined in the option specs
    UnknownLongOption(Field),

    /// Long option that matches more than one option spec
    ///
    /// The second item of the tuple is a list of all option specs that matched.
    AmbiguousLongOption(Field, Vec<&'a OptionSpec<'a>>),
}

impl std::fmt::Display for Error<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn long_option_name(field: &Field) -> &str {
            match field.value.find('=') {
                None => &field.value,
                Some(index) => &field.value[..index],
            }
        }

        use Error::*;
        match self {
            UnknownShortOption(c, _field) => write!(f, "unknown option {:?}", c),
            UnknownLongOption(field) => {
                write!(f, "unknown option {:?}", long_option_name(field))
            }
            AmbiguousLongOption(field, _specs) => {
                write!(f, "ambiguous option {:?}", long_option_name(field))
            }
        }
    }
}

impl std::error::Error for Error<'_> {}

/// Parses short options in an argument.
///
/// This function examines the first field yielded by `arguments` and consumes
/// it if it contains one or more short options. If the last option requires an
/// argument and the field does not include one, the following field is consumed
/// as the argument.
///
/// This function returns `Ok(true)` if consumed one or more fields.
fn parse_short_options<'a, I: Iterator<Item = Field>>(
    option_specs: &'a [OptionSpec<'a>],
    arguments: &mut Peekable<I>,
    option_occurrences: &mut Vec<OptionOccurrence<'a>>,
) -> Result<bool, Error<'a>> {
    fn starts_with_single_hyphen(field: &Field) -> bool {
        let mut chars = field.value.chars();
        chars.next() == Some('-') && !matches!(chars.next(), None | Some('-'))
    }

    let argument = match arguments.next_if(starts_with_single_hyphen) {
        None => return Ok(false),
        Some(argument) => argument,
    };

    let mut chars = argument.value.chars();
    chars.next(); // Skip the initial hyphen

    while let Some(c) = chars.next() {
        let spec = match option_specs.iter().find(|spec| spec.get_short() == Some(c)) {
            None => return Err(Error::UnknownShortOption(c, argument)),
            Some(spec) => spec,
        };
        match spec.get_argument() {
            OptionArgumentSpec::None => {
                let argument = None;
                option_occurrences.push(OptionOccurrence { spec, argument });
            }
            OptionArgumentSpec::Required => {
                let remainder_len = chars.as_str().len();
                let argument = if remainder_len == 0 {
                    // The option argument is the next command-line argument.
                    arguments.next()
                    // TODO Error if argument is none
                } else {
                    // The option argument is the rest of the current command-line argument.
                    let prefix = argument.value.len() - remainder_len;
                    let mut argument = argument;
                    argument.value.drain(..prefix);
                    Some(argument)
                };
                option_occurrences.push(OptionOccurrence { spec, argument });
                break;
            }
        };
    }
    Ok(true)
}

/// Finds an option spec that matches the given long option name.
///
/// Returns `Err(all_matched_options)` if there is no match or more than one match.
fn long_match<'a>(
    option_specs: &'a [OptionSpec<'a>],
    name: &str,
) -> Result<&'a OptionSpec<'a>, Vec<&'a OptionSpec<'a>>> {
    let mut matches = Vec::new();
    for spec in option_specs {
        match spec.long_match(name) {
            LongMatch::None => (),
            LongMatch::Partial => {
                matches.push(spec);
            }
            LongMatch::Exact => return Ok(spec),
        }
    }
    if matches.len() == 1 {
        Ok(matches[0])
    } else {
        Err(matches)
    }
}

/// Parses a long option.
///
/// This function examines the first field yielded by `arguments` and consumes
/// it if it is a long option. If the option requires an argument and the field
/// does not include a delimiting `=` sign, the following field is consumed as
/// the argument.
fn parse_long_option<'a, I: Iterator<Item = Field>>(
    option_specs: &'a [OptionSpec<'a>],
    arguments: &mut Peekable<I>,
) -> Result<Option<OptionOccurrence<'a>>, Error<'a>> {
    fn starts_with_double_hyphen(field: &Field) -> bool {
        match field.value.strip_prefix("--") {
            Some(body) => !body.is_empty(),
            None => false,
        }
    }

    let argument = match arguments.next_if(starts_with_double_hyphen) {
        Some(argument) => argument,
        None => return Ok(None),
    };

    let equal = argument.value.find('=');

    let name = match equal {
        Some(index) => &argument.value[2..index],
        None => &argument.value[2..],
    };

    let spec = match long_match(option_specs, name) {
        Ok(spec) => spec,
        Err(matched_specs) => {
            return Err(if matched_specs.is_empty() {
                Error::UnknownLongOption(argument)
            } else {
                Error::AmbiguousLongOption(argument, matched_specs)
            })
        }
    };

    let mut argument = equal.map(|index| {
        let mut argument = argument;
        argument.value.drain(..index + 1); // Remove "--", name, and "="
        argument
    });

    match (spec.get_argument(), &argument) {
        (OptionArgumentSpec::None, None) => (),
        (OptionArgumentSpec::None, Some(_)) => todo!("TODO: unexpected argument error"),
        (OptionArgumentSpec::Required, None) => {
            argument = arguments.next();
            // TODO: Error if none
        }
        (OptionArgumentSpec::Required, Some(_)) => (),
    }

    Ok(Some(OptionOccurrence { spec, argument }))
}

/// Parses command-line arguments into options and operands.
///
/// The first argument is always dropped and the remaining arguments are parsed.
///
/// If successful, returns a pair of option occurrences and operands.
pub fn parse_arguments<'a>(
    option_specs: &'a [OptionSpec<'a>],
    _mode: Mode,
    arguments: Vec<Field>,
) -> Result<(Vec<OptionOccurrence<'a>>, Vec<Field>), Error<'a>> {
    let mut arguments = arguments.into_iter().skip(1).peekable();

    let mut option_occurrences = vec![];
    loop {
        if parse_short_options(option_specs, &mut arguments, &mut option_occurrences)? {
            continue;
        }
        if let Some(occurrence) = parse_long_option(option_specs, &mut arguments)? {
            option_occurrences.push(occurrence);
            continue;
        }
        break;
    }

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
    fn multiple_occurrences_of_same_option_spec_short() {
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
    fn occurrences_of_multiple_option_specs_short() {
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
    fn non_occurring_long_option() {
        let specs = &[OptionSpec::new().long("option")];

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
    fn single_occurrence_of_long_option() {
        let specs = &[OptionSpec::new().long("option")];

        let arguments = Field::dummies(["foo", "--option"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", "--option", "bar"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(operands, Field::dummies(["bar"]));
    }

    #[test]
    fn multiple_occurrences_of_same_option_spec_long() {
        let specs = &[OptionSpec::new().long("foo")];

        let arguments = Field::dummies(["command", "--foo", "--foo"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("foo"));
        assert_eq!(options[1].spec.get_long(), Some("foo"));
        assert_eq!(operands, []);

        let arguments = Field::dummies(["command", "--foo", "--foo", "argument"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("foo"));
        assert_eq!(options[1].spec.get_long(), Some("foo"));
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn occurrences_of_multiple_option_specs_long() {
        let specs = &[OptionSpec::new().long("foo"), OptionSpec::new().long("bar")];

        let arguments = Field::dummies(["command", "--foo", "--bar", "!"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("foo"));
        assert_eq!(options[1].spec.get_long(), Some("bar"));
        assert_eq!(operands, Field::dummies(["!"]));

        let arguments = Field::dummies(["command", "--bar", "--foo", "--bar"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("bar"));
        assert_eq!(options[1].spec.get_long(), Some("foo"));
        assert_eq!(options[2].spec.get_long(), Some("bar"));
        assert_eq!(operands, []);
    }

    #[test]
    fn mixed_occurrences_of_various_option_specs() {
        let specs = &[
            OptionSpec::new().short('a'),
            OptionSpec::new().short('b').long("bar"),
            OptionSpec::new().long("baz"),
        ];

        let arguments = Field::dummies(["foo", "-ba", "--baz", "--bar", "--", "-a"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 4, "{:?}", options);
        assert_eq!(options[0].spec, &OptionSpec::new().short('b').long("bar"));
        assert_eq!(options[1].spec, &OptionSpec::new().short('a'));
        assert_eq!(options[2].spec, &OptionSpec::new().long("baz"));
        assert_eq!(options[3].spec, &OptionSpec::new().short('b').long("bar"));
        assert_eq!(operands, Field::dummies(["-a"]));
    }

    #[test]
    fn abbreviated_long_option_without_non_match() {
        let specs = &[OptionSpec::new().long("min")];

        let arguments = Field::dummies(["command", "--mi"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("min"));
        assert_eq!(operands, []);
    }

    #[test]
    fn abbreviated_long_option_with_non_match() {
        let specs = &[OptionSpec::new().long("max"), OptionSpec::new().long("min")];

        let arguments = Field::dummies(["command", "--mi"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("min"));
        assert_eq!(operands, []);
    }

    #[test]
    fn long_option_prefers_exact_match() {
        let specs = &[
            OptionSpec::new().long("many"),
            OptionSpec::new().long("man"),
            OptionSpec::new().long("manual"),
        ];

        let arguments = Field::dummies(["command", "--man"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("man"));
        assert_eq!(operands, []);
    }

    #[test]
    fn adjacent_argument_to_long_option() {
        let specs = &[OptionSpec::new()
            .long("option")
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["foo", "--option="]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "");
            assert_eq!(field.origin.line.value, "--option=");
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", "--option=x", "--option=value", "argument"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "x");
            assert_eq!(field.origin.line.value, "--option=x");
        });
        assert_eq!(options[1].spec.get_long(), Some("option"));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "value");
            assert_eq!(field.origin.line.value, "--option=value");
        });
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn separate_argument_to_long_option() {
        let specs = &[OptionSpec::new()
            .long("option")
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["foo", "--option", ""]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "");
            assert_eq!(field.origin.line.value, "");
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["foo", "--option", "x", "--option", "value", "argument"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "{:?}", options);
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "x");
            assert_eq!(field.origin.line.value, "x");
        });
        assert_eq!(options[1].spec.get_long(), Some("option"));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "value");
            assert_eq!(field.origin.line.value, "value");
        });
        assert_eq!(operands, Field::dummies(["argument"]));
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
    // TODO digit_options_are_recognized (depending mode)
    // TODO rejecting_non_portable_options (depending mode)

    #[test]
    fn unknown_short_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["foo", "-x"]);
        let error = parse_arguments(&[], Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, Error::UnknownShortOption('x', field) => {
            assert_eq!(field.value, "-x");
        });
        assert_eq!(error.to_string(), "unknown option 'x'");

        let arguments = Field::dummies(["foo", "-x"]);
        let error = parse_arguments(specs, Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, Error::UnknownShortOption('x', field) => {
            assert_eq!(field.value, "-x");
        });
        assert_eq!(error.to_string(), "unknown option 'x'");
    }

    #[test]
    fn unknown_long_option() {
        let specs = &[OptionSpec::new().long("one")];

        let arguments = Field::dummies(["foo", "--two"]);
        let error = parse_arguments(&[], Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, Error::UnknownLongOption(field) => {
            assert_eq!(field.value, "--two");
        });
        assert_eq!(error.to_string(), "unknown option \"--two\"");

        let arguments = Field::dummies(["foo", "--two=three"]);
        let error = parse_arguments(specs, Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, Error::UnknownLongOption(field) => {
            assert_eq!(field.value, "--two=three");
        });
        assert_eq!(error.to_string(), "unknown option \"--two\"");
    }

    #[test]
    fn ambiguous_long_option() {
        let specs = &[
            OptionSpec::new().long("max"),
            OptionSpec::new().long("min"),
            OptionSpec::new().long("value"),
        ];

        let arguments = Field::dummies(["command", "--m"]);
        let error = parse_arguments(specs, Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, Error::AmbiguousLongOption(field, matched_specs) => {
            assert_eq!(field.value, "--m");
            assert_eq!(matched_specs.as_slice(), [&specs[0], &specs[1]]);
        });
        assert_eq!(error.to_string(), "ambiguous option \"--m\"");
    }

    // TODO missing_argument_to_short_option
    // TODO missing_argument_to_long_option
    // TODO unexpected_argument_to_long_option
}
