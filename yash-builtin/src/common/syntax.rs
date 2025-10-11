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

//! Command-line argument syntax parser
//!
//! This module provides functionalities for parsing command-line arguments into
//! options and operands.
//!
//! This module's parser can parse command lines that adhere to [POSIX Utility
//! Syntax Guidelines] and support non-standard syntax extensions such as long
//! options and options after operands.
//!
//! [POSIX Utility Syntax Guidelines]: https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02
//!
//! # Usage
//!
//! To parse arguments, first create a list of [option specs](OptionSpec). Each
//! option spec describes a single possible option. Then call
//! [`parse_arguments`] with the option specs and the list of arguments to
//! parse. The function returns a pair of [option occurrences](OptionOccurrence)
//! and operands. In case of an error, the function returns a [`ParseError`].
//!
//! [`ConflictingOptionError`] is a helper object for constructing an error
//! message from a list of conflicting option occurrences. You need to
//! instantiate this object for yourself as this module does not provide a
//! function for detecting conflicting options.
//!
//! # Example
//!
//! ```
//! use yash_builtin::common::syntax::*;
//! let specs = &[
//!     OptionSpec::new().short('a'),
//!     OptionSpec::new().short('b').long("bar"),
//!     OptionSpec::new().long("baz").argument(OptionArgumentSpec::Required),
//! ];
//!
//! let arguments = Field::dummies(["-ba", "--baz", "--", "--bar", "--", "-a", "foo"]);
//! let (options, operands) = parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
//! assert_eq!(options.len(), 4);
//! assert_eq!(options[0].spec, &specs[1]); // 'b' in "-ba"
//! assert_eq!(options[0].argument, None);
//! assert_eq!(options[1].spec, &specs[0]); // 'a' in "-ba"
//! assert_eq!(options[1].argument, None);
//! assert_eq!(options[2].spec, &specs[2]); // "--baz"
//! assert_eq!(options[2].argument, Some(Field::dummy("--")));
//! assert_eq!(options[3].spec, &specs[1]); // "--bar"
//! assert_eq!(options[3].argument, None);
//! assert_eq!(operands, Field::dummies(["-a", "foo"]));
//! ```

use std::iter::Peekable;
use thiserror::Error;
#[allow(deprecated)]
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_syntax::source::pretty::{Report, ReportType, Snippet};
use yash_syntax::source::{
    Location,
    pretty::{Span, SpanRole, add_span},
};

#[doc(no_inline)]
pub use yash_env::semantics::Field;

/// Specification for an options's argument
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum OptionArgumentSpec {
    /// The option does not take an argument. (default)
    #[default]
    None,
    /// The option requires an argument.
    Required,
    // /// The option may have an argument.
    // Optional,
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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

#[test]
fn new_option_spec_eq_default() {
    assert_eq!(OptionSpec::new(), OptionSpec::default());
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

/// Returns the option name like `-f` or `--foo`.
///
/// If the spec has both short and long names, the result is like `-f/--foo`.
/// If the spec has neither of them, the result is `?`.
impl std::fmt::Display for OptionSpec<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(short) = self.short {
            write!(f, "-{short}")?;
            if let Some(long) = self.long {
                write!(f, "/--{long}")?;
            }
            Ok(())
        } else if let Some(long) = self.long {
            write!(f, "--{long}")
        } else {
            write!(f, "?")
        }
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

/// Configuration for customizing the argument parsing behavior
///
/// # Examples
///
/// The default configuration disables all non-portable extensions:
///
/// ```
/// # use yash_builtin::common::syntax::Mode;
/// let mode = Mode::default();
/// assert!(!mode.accepts_long_options());
/// # // TODO other properties
/// ```
///
/// The [`with_extensions`](Self::with_extensions) function returns a `Mode`
/// with those extensions enabled.
///
/// ```
/// # use yash_builtin::common::syntax::Mode;
/// let mode = Mode::with_extensions();
/// assert!(mode.accepts_long_options());
/// # // TODO other properties
/// ```
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct Mode {
    long_options: bool,
    // TODO Change long_options to non_portable_option_names
    // TODO options_after_operands
    // TODO negative_integer_operands
}

impl Mode {
    /// Returns a new `Mode` with non-portable extensions enabled.
    pub const fn with_extensions() -> Self {
        Mode { long_options: true }
    }

    /// Convenience initializer
    ///
    /// This function returns `Self::default()` or `Self::with_extensions()`
    /// depending on `env.options.get(PosixlyCorrect)`.
    pub fn with_env(env: &yash_env::Env) -> Self {
        use yash_env::option::{Off, On, PosixlyCorrect};
        match env.options.get(PosixlyCorrect) {
            On => Self::default(),
            Off => Self::with_extensions(),
        }
    }

    /// Whether the parser accepts long options or not
    pub const fn accepts_long_options(&self) -> bool {
        self.long_options
    }

    /// Sets whether the parser accepts long options or not.
    pub fn accept_long_options(&mut self, accept: bool) -> &mut Self {
        self.long_options = accept;
        self
    }
}

/// Occurrence of an option
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct OptionOccurrence<'a> {
    /// Specification for this option
    pub spec: &'a OptionSpec<'a>,

    /// Location of the field containing this option
    pub location: Location,

    /// Argument to this option
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
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseError<'a> {
    /// Short option that is not defined in the option specs
    #[error("unknown option {0:?}")]
    UnknownShortOption(char, Field),

    /// Long option that is not defined in the option specs
    #[error("unknown option {:?}", long_option_name(.0))]
    UnknownLongOption(Field),

    // TODO Change this to NonPortableOptionName
    /// Long option that is defined in an option spec but disabled by
    /// configuration ([`Mode`]).
    #[error("unsupported option {:?}", .0.value)]
    UnsupportedLongOption(Field, &'a OptionSpec<'a>),

    /// Long option that matches more than one option spec
    ///
    /// The second item of the tuple is a list of all option specs that matched.
    #[error("ambiguous option {:?}", long_option_name(.0))]
    AmbiguousLongOption(Field, Vec<&'a OptionSpec<'a>>),

    /// Option missing its required argument
    #[error("option {:?} missing an argument", .0.value)]
    MissingOptionArgument(Field, &'a OptionSpec<'a>),

    /// Long option having an unexpected argument
    #[error("option {:?} with an unexpected argument", .0.value)]
    UnexpectedOptionArgument(Field, &'a OptionSpec<'a>),
}

fn long_option_name(field: &Field) -> &str {
    match field.value.find('=') {
        None => &field.value,
        Some(index) => &field.value[..index],
    }
}

impl ParseError<'_> {
    /// Returns a reference to the field in which the error occurred.
    pub fn field(&self) -> &Field {
        use ParseError::*;
        match self {
            UnknownShortOption(_char, field) => field,
            UnknownLongOption(field) => field,
            UnsupportedLongOption(field, _spec) => field,
            AmbiguousLongOption(field, _specs) => field,
            MissingOptionArgument(field, _spec) => field,
            UnexpectedOptionArgument(field, _spec) => field,
        }
    }

    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let field = self.field();
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = Snippet::with_primary_span(&field.origin, field.value.as_str().into());
        // TODO provide more info about the erroneous option
        report
    }
}

impl<'a> From<&'a ParseError<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a ParseError<'a>) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for ParseError<'_> {
    fn message_title(&self) -> std::borrow::Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let field = self.field();
        Annotation::new(
            AnnotationType::Error,
            field.value.as_str().into(),
            &field.origin,
        )
    }

    // TODO provide more info about the erroneous option
    // fn footers(&self) -> Vec<Footer> {}
}

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
) -> Result<bool, ParseError<'a>> {
    fn starts_with_single_hyphen(field: &Field) -> bool {
        let mut chars = field.value.chars();
        chars.next() == Some('-') && !matches!(chars.next(), None | Some('-'))
    }

    let field = match arguments.next_if(starts_with_single_hyphen) {
        None => return Ok(false),
        Some(field) => field,
    };

    let mut chars = field.value.chars();
    chars.next(); // Skip the initial hyphen

    while let Some(c) = chars.next() {
        let spec = match option_specs.iter().find(|spec| spec.get_short() == Some(c)) {
            None => return Err(ParseError::UnknownShortOption(c, field)),
            Some(spec) => spec,
        };
        match spec.get_argument() {
            OptionArgumentSpec::None => {
                option_occurrences.push(OptionOccurrence {
                    spec,
                    location: field.origin.clone(),
                    argument: None,
                });
            }
            OptionArgumentSpec::Required => {
                let remainder_len = chars.as_str().len();
                let location = field.origin.clone();
                let argument = if remainder_len == 0 {
                    // The option argument is the next command-line argument.
                    arguments
                        .next()
                        .ok_or(ParseError::MissingOptionArgument(field, spec))?
                } else {
                    // The option argument is the rest of the current command-line argument.
                    let prefix = field.value.len() - remainder_len;
                    let mut field = field;
                    field.value.drain(..prefix);
                    field
                };
                option_occurrences.push(OptionOccurrence {
                    spec,
                    location,
                    argument: Some(argument),
                });
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
    mode: Mode,
    arguments: &mut Peekable<I>,
) -> Result<Option<OptionOccurrence<'a>>, ParseError<'a>> {
    fn starts_with_double_hyphen(field: &Field) -> bool {
        match field.value.strip_prefix("--") {
            Some(body) => !body.is_empty(),
            None => false,
        }
    }

    let field = match arguments.next_if(starts_with_double_hyphen) {
        Some(field) => field,
        None => return Ok(None),
    };

    let equal = field.value.find('=');

    let name = match equal {
        Some(index) => &field.value[2..index],
        None => &field.value[2..],
    };

    let spec = match long_match(option_specs, name) {
        Ok(spec) if mode.accepts_long_options() => spec,
        Ok(spec) => return Err(ParseError::UnsupportedLongOption(field, spec)),
        Err(matched_specs) => {
            return Err(if matched_specs.is_empty() {
                ParseError::UnknownLongOption(field)
            } else {
                ParseError::AmbiguousLongOption(field, matched_specs)
            });
        }
    };

    let location = field.origin.clone();

    let argument = match (spec.get_argument(), equal) {
        (OptionArgumentSpec::None, None) => None,
        (OptionArgumentSpec::None, Some(_)) => {
            return Err(ParseError::UnexpectedOptionArgument(field, spec));
        }
        (OptionArgumentSpec::Required, None) => {
            let argument = arguments.next();
            if argument.is_none() {
                return Err(ParseError::MissingOptionArgument(field, spec));
            }
            argument
        }
        (OptionArgumentSpec::Required, Some(index)) => {
            let mut field = field;
            field.value.drain(..index + 1); // Remove "--", name, and "="
            Some(field)
        }
    };

    Ok(Some(OptionOccurrence {
        spec,
        location,
        argument,
    }))
}

/// Parses command-line arguments into options and operands.
///
/// The arguments should not include a leading command name field.
///
/// If successful, returns a pair of option occurrences and operands.
pub fn parse_arguments<'a>(
    option_specs: &'a [OptionSpec<'a>],
    mode: Mode,
    arguments: Vec<Field>,
) -> Result<(Vec<OptionOccurrence<'a>>, Vec<Field>), ParseError<'a>> {
    let mut arguments = arguments.into_iter().peekable();

    let mut option_occurrences = vec![];
    loop {
        if parse_short_options(option_specs, &mut arguments, &mut option_occurrences)? {
            continue;
        }
        if let Some(occurrence) = parse_long_option(option_specs, mode, &mut arguments)? {
            option_occurrences.push(occurrence);
            continue;
        }
        break;
    }

    arguments.next_if(|argument| argument.value == "--");

    let operands = arguments.collect();
    Ok((option_occurrences, operands))
}

/// Error indicating that two or more options conflict with each other
///
/// This is a helper object for constructing an error message from a list of
/// conflicting option occurrences. An instance of this type can be created
/// using [`new`](Self::new) or [`pick_from_indexes`](Self::pick_from_indexes)
/// and printed with [`report_error`](crate::common::report::report_error).
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("conflicting options")]
pub struct ConflictingOptionError<'a> {
    options: Vec<OptionOccurrence<'a>>,
}

impl<'a> ConflictingOptionError<'a> {
    /// Creates a new `ConflictingOptionError` from a list of conflicting options.
    ///
    /// The vector should contain at least two elements, or the returned error
    /// object may panic when formatted.
    #[must_use]
    pub fn new<T: Into<Vec<OptionOccurrence<'a>>>>(options: T) -> Self {
        let options = options.into();
        Self { options }
    }

    /// Creates a new `ConflictingOptionError` with conflicting options
    /// extracted from a vector.
    ///
    /// This function retains only the options in the vector whose indexes are
    /// specified in `indexes`. The other options are discarded.
    ///
    /// The `indexes` may be specified in any order as they are sorted in this
    /// function.
    ///
    /// `indexes` should contain at least two elements, or the returned error
    /// object may panic when formatted. This function panics immediately if
    /// `indexes` contains a duplicate index.
    ///
    /// This function is useful for constructing a `ConflictingOptionError` from
    /// the result of [`parse_arguments`].
    /// After examining the `OptionOccurrence` vector returned by the function,
    /// the caller can pick the indexes of the conflicting options and pass them
    /// to this function.
    ///
    /// For example, calling `ConflictingOptionError::pick_from_indexes(vec![a,
    /// b, c, d, e], [3, 0])` is equivalent to `ConflictingOptionError::new([a,
    /// d])`.
    #[must_use]
    pub fn pick_from_indexes<const N: usize>(
        mut options: Vec<OptionOccurrence<'a>>,
        mut indexes: [usize; N],
    ) -> Self {
        indexes.sort();

        // Remove the options that are not picked.
        let mut option_index = 0;
        let mut index_index = 0;
        options.retain(|_| {
            if index_index >= N {
                return false;
            }
            assert!(
                option_index <= indexes[index_index],
                "duplicate index {}",
                indexes[index_index]
            );
            let pick = option_index == indexes[index_index];
            option_index += 1;
            if pick {
                index_index += 1;
            }
            pick
        });

        Self { options }
    }

    /// Returns the list of conflicting options.
    #[must_use]
    pub fn options(&self) -> &[OptionOccurrence<'a>] {
        &self.options
    }

    /// Converts this error into a report.
    #[must_use]
    pub fn to_report(&'a self) -> Report<'a> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = Snippet::with_primary_span(
            &self.options[0].location,
            format!("the {} option ...", &self.options[0].spec).into(),
        );
        for option in &self.options[1..] {
            let span = Span {
                range: option.location.byte_range(),
                role: SpanRole::Primary {
                    label: format!("... cannot be used with the {} option", &option.spec).into(),
                },
            };
            add_span(&option.location.code, span, &mut report.snippets);
        }
        report
    }
}

impl<'a> From<Vec<OptionOccurrence<'a>>> for ConflictingOptionError<'a> {
    /// Creates a new `ConflictingOptionError` from a list of conflicting options.
    ///
    /// The vector should contain at least two elements, or the returned error
    /// object may panic when formatted.
    fn from(options: Vec<OptionOccurrence<'a>>) -> Self {
        ConflictingOptionError { options }
    }
}

impl<'a> From<ConflictingOptionError<'a>> for Vec<OptionOccurrence<'a>> {
    fn from(error: ConflictingOptionError<'a>) -> Self {
        error.options
    }
}

impl<'a> From<&'a ConflictingOptionError<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a ConflictingOptionError<'a>) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for ConflictingOptionError<'_> {
    fn message_title(&self) -> std::borrow::Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let label = format!("the {} option ...", &self.options[0].spec).into();
        let location = &self.options[0].location;
        Annotation::new(AnnotationType::Error, label, location)
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        results.extend(self.options[1..].iter().map(|option| {
            let label = format!("... cannot be used with the {} option", &option.spec).into();
            let location = &option.location;
            Annotation::new(AnnotationType::Error, label, location)
        }))
    }
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
        let arguments = Field::dummies([""]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies([""]));

        let arguments = Field::dummies(["foo", "bar", "", "baz"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["foo", "bar", "", "baz"]));
    }

    #[test]
    fn operands_following_separator() {
        let arguments = Field::dummies(["--"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);

        let arguments = Field::dummies(["--", "1"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["1"]));

        let arguments = Field::dummies(["--", "a", "", "z"]);
        let (options, operands) = parse_arguments(&[], Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["a", "", "z"]));
    }

    #[test]
    fn non_occurring_short_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = vec![];
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);

        let arguments = Field::dummies([""]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies([""]));
    }

    #[test]
    fn single_occurrence_of_short_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["-a"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(options[0].location, Location::dummy("-a"));
        assert_eq!(options[0].argument, None);
        assert_eq!(operands, []);

        let arguments = Field::dummies(["-a", "foo"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(options[0].location, Location::dummy("-a"));
        assert_eq!(options[0].argument, None);
        assert_eq!(operands, Field::dummies(["foo"]));
    }

    #[test]
    fn multiple_occurrences_of_same_option_spec_short() {
        let specs = &[OptionSpec::new().short('b')];

        let arguments = Field::dummies(["-b", "-b"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('b'));
        assert_eq!(options[1].spec.get_short(), Some('b'));
        assert_eq!(operands, []);

        let arguments = Field::dummies(["-b", "-b", "argument"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('b'));
        assert_eq!(options[1].spec.get_short(), Some('b'));
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn occurrences_of_multiple_option_specs_short() {
        let specs = &[OptionSpec::new().short('x'), OptionSpec::new().short('y')];

        let arguments = Field::dummies(["-x", "-y", "!"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('x'));
        assert_eq!(options[1].spec.get_short(), Some('y'));
        assert_eq!(operands, Field::dummies(["!"]));

        let arguments = Field::dummies(["-y", "-x", "-y"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('y'));
        assert_eq!(options[1].spec.get_short(), Some('x'));
        assert_eq!(options[2].spec.get_short(), Some('y'));
        assert_eq!(operands, []);
    }

    #[test]
    fn multiple_occurrences_of_short_options_in_single_argument() {
        let specs = &[OptionSpec::new().short('p'), OptionSpec::new().short('q')];

        let arguments = Field::dummies(["-pq", "!"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('p'));
        assert_eq!(options[0].location, Location::dummy("-pq"));
        assert_eq!(options[1].spec.get_short(), Some('q'));
        assert_eq!(options[1].location, Location::dummy("-pq"));
        assert_eq!(operands, Field::dummies(["!"]));

        let arguments = Field::dummies(["-qpq"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('q'));
        assert_eq!(options[1].spec.get_short(), Some('p'));
        assert_eq!(options[2].spec.get_short(), Some('q'));
        assert_eq!(operands, []);
    }

    #[test]
    fn single_hyphen_argument_is_not_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["-"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["-"]));

        let arguments = Field::dummies(["-", "-"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies(["-", "-"]));
    }

    #[test]
    fn options_are_not_recognized_after_separator() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["-a", "--", "-a", "--", "-a"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(operands, Field::dummies(["-a", "--", "-a"]));
    }

    #[test]
    fn options_are_not_recognized_after_operand_by_default() {
        let specs = &[OptionSpec::new().short('x'), OptionSpec::new().short('y')];

        let arguments = Field::dummies(["-x", "foo", "-y", "bar"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('x'));
        assert_eq!(operands, Field::dummies(["foo", "-y", "bar"]));
    }

    #[test]
    fn adjacent_argument_to_short_option() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["-afoo"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(options[0].location, Location::dummy("-afoo"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "foo");
            assert_eq!(field.origin, Location::dummy("-afoo"));
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["-a1", "-a2", "3"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "1");
            assert_eq!(field.origin, Location::dummy("-a1"));
        });
        assert_eq!(options[1].spec.get_short(), Some('a'));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "2");
            assert_eq!(field.origin, Location::dummy("-a2"));
        });
        assert_eq!(operands, Field::dummies(["3"]));
    }

    #[test]
    fn separate_argument_to_short_option() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["-a", "foo"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "foo");
            assert_eq!(field.origin, Location::dummy("foo"));
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["-a", "1", "-a", "2", "3"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "1");
            assert_eq!(field.origin, Location::dummy("1"));
        });
        assert_eq!(options[1].spec.get_short(), Some('a'));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "2");
            assert_eq!(field.origin, Location::dummy("2"));
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

        let arguments = Field::dummies(["-abcdef"]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 3, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_eq!(options[0].argument, None);
        assert_eq!(options[1].spec.get_short(), Some('b'));
        assert_eq!(options[1].argument, None);
        assert_eq!(options[2].spec.get_short(), Some('c'));
        assert_matches!(options[2].argument, Some(ref field) => {
            assert_eq!(field.value, "def");
            assert_eq!(field.origin, Location::dummy("-abcdef"));
        });
        assert_eq!(operands, []);
    }

    #[test]
    fn empty_argument_to_short_option() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["-a", ""]);
        let (options, operands) = parse_arguments(specs, Mode::default(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "");
            assert_eq!(field.origin, Location::dummy(""));
        });
        assert_eq!(operands, []);
    }

    #[test]
    fn non_occurring_long_option() {
        let specs = &[OptionSpec::new().long("option")];

        let arguments = vec![];
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, []);

        let arguments = Field::dummies([""]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options, []);
        assert_eq!(operands, Field::dummies([""]));
    }

    #[test]
    fn single_occurrence_of_long_option() {
        let specs = &[OptionSpec::new().long("option")];

        let arguments = Field::dummies(["--option"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(options[0].location, Location::dummy("--option"));
        assert_eq!(options[0].argument, None);
        assert_eq!(operands, []);

        let arguments = Field::dummies(["--option", "foo"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(options[0].location, Location::dummy("--option"));
        assert_eq!(options[0].argument, None);
        assert_eq!(operands, Field::dummies(["foo"]));
    }

    #[test]
    fn multiple_occurrences_of_same_option_spec_long() {
        let specs = &[OptionSpec::new().long("foo")];

        let arguments = Field::dummies(["--foo", "--foo"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("foo"));
        assert_eq!(options[1].spec.get_long(), Some("foo"));
        assert_eq!(operands, []);

        let arguments = Field::dummies(["--foo", "--foo", "argument"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("foo"));
        assert_eq!(options[1].spec.get_long(), Some("foo"));
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn occurrences_of_multiple_option_specs_long() {
        let specs = &[OptionSpec::new().long("foo"), OptionSpec::new().long("bar")];

        let arguments = Field::dummies(["--foo", "--bar", "!"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("foo"));
        assert_eq!(options[1].spec.get_long(), Some("bar"));
        assert_eq!(operands, Field::dummies(["!"]));

        let arguments = Field::dummies(["--bar", "--foo", "--bar"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 3, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("bar"));
        assert_eq!(options[1].spec.get_long(), Some("foo"));
        assert_eq!(options[2].spec.get_long(), Some("bar"));
        assert_eq!(operands, []);
    }

    #[test]
    fn abbreviated_long_option_without_non_match() {
        let specs = &[OptionSpec::new().long("min")];

        let arguments = Field::dummies(["--mi"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("min"));
        assert_eq!(operands, []);
    }

    #[test]
    fn abbreviated_long_option_with_non_match() {
        let specs = &[OptionSpec::new().long("max"), OptionSpec::new().long("min")];

        let arguments = Field::dummies(["--mi"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
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

        let arguments = Field::dummies(["--man"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("man"));
        assert_eq!(operands, []);
    }

    #[test]
    fn adjacent_argument_to_long_option() {
        let specs = &[OptionSpec::new()
            .long("option")
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["--option="]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(options[0].location, Location::dummy("--option="));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "");
            assert_eq!(field.origin, Location::dummy("--option="));
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["--option=x", "--option=value", "argument"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(options[0].location, Location::dummy("--option=x"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "x");
            assert_eq!(field.origin, Location::dummy("--option=x"));
        });
        assert_eq!(options[1].spec.get_long(), Some("option"));
        assert_eq!(options[1].location, Location::dummy("--option=value"));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "value");
            assert_eq!(field.origin, Location::dummy("--option=value"));
        });
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn separate_argument_to_long_option() {
        let specs = &[OptionSpec::new()
            .long("option")
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["--option", ""]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 1, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(options[0].location, Location::dummy("--option"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "");
            assert_eq!(field.origin, Location::dummy(""));
        });
        assert_eq!(operands, []);

        let arguments = Field::dummies(["--option", "x", "--option", "value", "argument"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_long(), Some("option"));
        assert_eq!(options[0].location, Location::dummy("--option"));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "x");
            assert_eq!(field.origin, Location::dummy("x"));
        });
        assert_eq!(options[1].spec.get_long(), Some("option"));
        assert_eq!(options[1].location, Location::dummy("--option"));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "value");
            assert_eq!(field.origin, Location::dummy("value"));
        });
        assert_eq!(operands, Field::dummies(["argument"]));
    }

    #[test]
    fn option_argument_that_looks_like_separator() {
        let specs = &[OptionSpec::new()
            .short('a')
            .argument(OptionArgumentSpec::Required)];

        let arguments = Field::dummies(["-a", "argument", "-a", "--", "--", "operand"]);
        let (options, operands) =
            parse_arguments(specs, Mode::with_extensions(), arguments).unwrap();
        assert_eq!(options.len(), 2, "options = {options:?}");
        assert_eq!(options[0].spec.get_short(), Some('a'));
        assert_matches!(options[0].argument, Some(ref field) => {
            assert_eq!(field.value, "argument");
            assert_eq!(field.origin, Location::dummy("argument"));
        });
        assert_eq!(options[1].spec.get_short(), Some('a'));
        assert_matches!(options[1].argument, Some(ref field) => {
            assert_eq!(field.value, "--");
            assert_eq!(field.origin, Location::dummy("--"));
        });
        assert_eq!(operands, Field::dummies(["operand"]));
    }

    // TODO options_are_recognized_after_operand (depending mode)
    // TODO digit_options_are_recognized (depending mode)
    // TODO rejecting_non_portable_options (depending mode)

    #[test]
    fn unknown_short_option() {
        let specs = &[OptionSpec::new().short('a')];

        let arguments = Field::dummies(["-x"]);
        let error = parse_arguments(&[], Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, ParseError::UnknownShortOption('x', field) => {
            assert_eq!(field.value, "-x");
        });
        assert_eq!(error.to_string(), "unknown option 'x'");

        let arguments = Field::dummies(["-x"]);
        let error = parse_arguments(specs, Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, ParseError::UnknownShortOption('x', field) => {
            assert_eq!(field.value, "-x");
        });
        assert_eq!(error.to_string(), "unknown option 'x'");
    }

    #[test]
    fn unknown_long_option() {
        let specs = &[OptionSpec::new().long("one")];

        let arguments = Field::dummies(["--two"]);
        let error = parse_arguments(&[], Mode::with_extensions(), arguments).unwrap_err();
        assert_matches!(&error, ParseError::UnknownLongOption(field) => {
            assert_eq!(field.value, "--two");
        });
        assert_eq!(error.to_string(), "unknown option \"--two\"");

        let arguments = Field::dummies(["--two=three"]);
        let error = parse_arguments(specs, Mode::with_extensions(), arguments).unwrap_err();
        assert_matches!(&error, ParseError::UnknownLongOption(field) => {
            assert_eq!(field.value, "--two=three");
        });
        assert_eq!(error.to_string(), "unknown option \"--two\"");
    }

    #[test]
    fn disabled_long_option() {
        let specs = &[OptionSpec::new().long("option")];

        let mode = *Mode::with_extensions().accept_long_options(false);
        let arguments = Field::dummies(["--option"]);
        let error = parse_arguments(specs, mode, arguments).unwrap_err();
        assert_matches!(&error, &ParseError::UnsupportedLongOption(ref field, spec) => {
            assert_eq!(field.value, "--option");
            assert_eq!(spec, &specs[0]);
        });
        assert_eq!(error.to_string(), "unsupported option \"--option\"");
    }

    #[test]
    fn ambiguous_long_option() {
        let specs = &[
            OptionSpec::new().long("max"),
            OptionSpec::new().long("min"),
            OptionSpec::new().long("value"),
        ];

        let arguments = Field::dummies(["--m"]);
        let error = parse_arguments(specs, Mode::with_extensions(), arguments).unwrap_err();
        assert_matches!(&error, ParseError::AmbiguousLongOption(field, matched_specs) => {
            assert_eq!(field.value, "--m");
            assert_eq!(matched_specs.as_slice(), [&specs[0], &specs[1]]);
        });
        assert_eq!(error.to_string(), "ambiguous option \"--m\"");
    }

    #[test]
    fn missing_argument_to_short_option() {
        use OptionArgumentSpec::Required;
        let specs = &[
            OptionSpec::new().short('a').argument(Required),
            OptionSpec::new().short('b'),
        ];

        let arguments = Field::dummies(["-a"]);
        let error = parse_arguments(specs, Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, &ParseError::MissingOptionArgument(ref field, spec) => {
            assert_eq!(field.value, "-a");
            assert_eq!(spec, &specs[0]);
        });
        assert_eq!(error.to_string(), "option \"-a\" missing an argument");

        let arguments = Field::dummies(["-ba"]);
        let error = parse_arguments(specs, Mode::default(), arguments).unwrap_err();
        assert_matches!(&error, &ParseError::MissingOptionArgument(ref field, spec) => {
            assert_eq!(field.value, "-ba");
            assert_eq!(spec, &specs[0]);
        });
        assert_eq!(error.to_string(), "option \"-ba\" missing an argument");
    }

    #[test]
    fn missing_argument_to_long_option() {
        use OptionArgumentSpec::Required;
        let specs = &[
            OptionSpec::new().long("foo").argument(Required),
            OptionSpec::new().long("bar"),
        ];

        let arguments = Field::dummies(["--fo"]);
        let error = parse_arguments(specs, Mode::with_extensions(), arguments).unwrap_err();
        assert_matches!(&error, &ParseError::MissingOptionArgument(ref field, spec) => {
            assert_eq!(field.value, "--fo");
            assert_eq!(spec, &specs[0]);
        });
        assert_eq!(error.to_string(), "option \"--fo\" missing an argument");
    }

    #[test]
    fn unexpected_argument_to_long_option() {
        use OptionArgumentSpec::Required;
        let specs = &[
            OptionSpec::new().long("foo").argument(Required),
            OptionSpec::new().long("bar"),
        ];

        let arguments = Field::dummies(["--bar=baz"]);
        let error = parse_arguments(specs, Mode::with_extensions(), arguments).unwrap_err();
        assert_matches!(&error, &ParseError::UnexpectedOptionArgument(ref field, spec) => {
            assert_eq!(field.value, "--bar=baz");
            assert_eq!(spec, &specs[1]);
        });
        assert_eq!(
            error.to_string(),
            "option \"--bar=baz\" with an unexpected argument"
        );
    }

    const OPTION_SPEC_A: OptionSpec = OptionSpec::new().short('a');
    const OPTION_SPEC_B: OptionSpec = OptionSpec::new().short('b');
    const OPTION_SPEC_C: OptionSpec = OptionSpec::new().short('c');
    const OPTION_SPEC_D: OptionSpec = OptionSpec::new().short('d');
    const OPTION_SPEC_E: OptionSpec = OptionSpec::new().short('e');

    fn dummy_options() -> Vec<OptionOccurrence<'static>> {
        vec![
            OptionOccurrence {
                spec: &OPTION_SPEC_A,
                location: Location::dummy("-a"),
                argument: None,
            },
            OptionOccurrence {
                spec: &OPTION_SPEC_B,
                location: Location::dummy("-b"),
                argument: None,
            },
            OptionOccurrence {
                spec: &OPTION_SPEC_C,
                location: Location::dummy("-c"),
                argument: None,
            },
            OptionOccurrence {
                spec: &OPTION_SPEC_D,
                location: Location::dummy("-d"),
                argument: None,
            },
            OptionOccurrence {
                spec: &OPTION_SPEC_E,
                location: Location::dummy("-e"),
                argument: None,
            },
        ]
    }

    #[test]
    fn pick_from_2_indexes() {
        let result = ConflictingOptionError::pick_from_indexes(dummy_options(), [1, 3]);
        let options = Vec::from(result);
        assert_matches!(options.as_slice(), [b, d] => {
            assert_eq!(b.spec, &OPTION_SPEC_B);
            assert_eq!(d.spec, &OPTION_SPEC_D);
        });
    }

    #[test]
    fn pick_from_2_indexes_reversed() {
        let result = ConflictingOptionError::pick_from_indexes(dummy_options(), [3, 1]);
        let options = Vec::from(result);
        assert_matches!(options.as_slice(), [b, d] => {
            assert_eq!(b.spec, &OPTION_SPEC_B);
            assert_eq!(d.spec, &OPTION_SPEC_D);
        });
    }

    #[test]
    fn pick_from_3_indexes() {
        let result = ConflictingOptionError::pick_from_indexes(dummy_options(), [0, 2, 4]);
        let options = Vec::from(result);
        assert_matches!(options.as_slice(), [a, c, e] => {
            assert_eq!(a.spec, &OPTION_SPEC_A);
            assert_eq!(c.spec, &OPTION_SPEC_C);
            assert_eq!(e.spec, &OPTION_SPEC_E);
        });
    }

    #[test]
    fn pick_from_4_indexes_shuffled() {
        let result = ConflictingOptionError::pick_from_indexes(dummy_options(), [3, 0, 4, 2]);
        let options = Vec::from(result);
        assert_matches!(options.as_slice(), [a, c, d, e] => {
            assert_eq!(a.spec, &OPTION_SPEC_A);
            assert_eq!(c.spec, &OPTION_SPEC_C);
            assert_eq!(d.spec, &OPTION_SPEC_D);
            assert_eq!(e.spec, &OPTION_SPEC_E);
        });
    }

    #[test]
    #[should_panic(expected = "duplicate index 1")]
    fn pick_from_duplicate_indexes() {
        let result = ConflictingOptionError::pick_from_indexes(dummy_options(), [1, 1]);
        unreachable!("{result:?}");
    }
}
