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

//! Command line argument parser for the typeset built-in
//!
//! There are two main functions in this module: [`parse`] and [`interpret`].
//! The former parses command line arguments into [`OptionOccurrence`]s and
//! operands, and the latter interprets them into a [`Command`].

use super::*;
use crate::common::syntax::Mode;
use std::iter::Peekable;
use thiserror::Error;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::source::Location;

/// Attribute that can be set on a variable or function
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Attr {
    ReadOnly,
    Export,
}

/// Dummy error returned when an `Attr` cannot be converted to a `FunctionAttr`
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UnsupportedAttr;

impl TryFrom<Attr> for VariableAttr {
    // An attribute that cannot be converted to a variable attribute may be
    // added in the future, so we don't use `Infallible` here.
    // type Error = Infallible;
    type Error = UnsupportedAttr;

    fn try_from(attr: Attr) -> Result<Self, Self::Error> {
        match attr {
            Attr::ReadOnly => Ok(Self::ReadOnly),
            Attr::Export => Ok(Self::Export),
        }
    }
}

impl TryFrom<Attr> for FunctionAttr {
    type Error = UnsupportedAttr;

    fn try_from(attr: Attr) -> Result<Self, Self::Error> {
        match attr {
            Attr::ReadOnly => Ok(Self::ReadOnly),
            Attr::Export => Err(UnsupportedAttr),
        }
    }
}

/// Specification of an option
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionSpec<'a> {
    /// Short option name
    pub short: char,
    /// Long option name (not including the leading `--`)
    pub long: &'a str,
    /// Attribute specified by this option
    pub attr: Option<Attr>,
}

impl std::fmt::Display for OptionSpec<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "-{}/--{}", self.short, self.long)
    }
}

/// Specification of the `-f`/`--functions` option
pub const FUNCTIONS_OPTION: OptionSpec<'static> = OptionSpec {
    short: 'f',
    long: "functions",
    attr: None,
};
/// Specification of the `-g`/`--global` option
pub const GLOBAL_OPTION: OptionSpec<'static> = OptionSpec {
    short: 'g',
    long: "global",
    attr: None,
};
/// Specification of the `-p`/`--print` option
pub const PRINT_OPTION: OptionSpec<'static> = OptionSpec {
    short: 'p',
    long: "print",
    attr: None,
};
/// Specification of the `-r`/`--readonly` option
pub const READONLY_OPTION: OptionSpec<'static> = OptionSpec {
    short: 'r',
    long: "readonly",
    attr: Some(Attr::ReadOnly),
};
/// Specification of the `-x`/`--export` option
pub const EXPORT_OPTION: OptionSpec<'static> = OptionSpec {
    short: 'x',
    long: "export",
    attr: Some(Attr::Export),
};
/// Specification of the `-X`/`--unexport` option
///
/// This option is deprecated.
pub const UNEXPORT_OPTION: OptionSpec<'static> = OptionSpec {
    short: 'X',
    long: "unexport",
    attr: None,
};

/// List of all option specifications applicable to the typeset built-in
pub const ALL_OPTIONS: &[OptionSpec<'static>] = &[
    FUNCTIONS_OPTION,
    GLOBAL_OPTION,
    PRINT_OPTION,
    READONLY_OPTION,
    EXPORT_OPTION,
    UNEXPORT_OPTION,
];

/// Occurrence of an option
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionOccurrence<'a> {
    /// Specification for this option
    pub spec: &'a OptionSpec<'a>,
    /// Whether this option is negated
    pub state: State,
    /// Location of the field containing this option
    pub location: Location,
}

/// Error in command line parsing
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ParseError {
    /// Short option that is not defined in the option specs
    #[error("unknown option {0:?}")]
    UnknownShortOption(char, Field),

    /// Long option that is not defined in the option specs
    #[error("unknown option {:?}", .0.value)]
    UnknownLongOption(Field),

    /// Long option that matches the prefix of more than one option name.
    #[error("ambiguous option name {:?}", .0.value)]
    AmbiguousLongOption(Field),

    /// Long option used while POSIX portability is required
    ///
    /// POSIX does not specify long options at all, so any `--name` or `++name`
    /// form is rejected while [`Mode::non_portable_option_names`] is `false`.
    /// This error applies to the `--` and `++` prefixes only; the `--`
    /// separator and short options remain acceptable. An option that cannot be
    /// canceled is still reported as
    /// [`UncancelableLongOption`](Self::UncancelableLongOption), since that
    /// error stands regardless of the mode.
    #[error("non-portable option {:?}", .0.value)]
    NonPortableLongOption(Field),

    /// Negated short option that is not an attribute
    #[error("option {0:?} cannot be canceled with '+'")]
    UncancelableShortOption(char, Field),

    /// Negated long option that is not an attribute
    #[error("option {:?} cannot be canceled with '++'", .0.value)]
    UncancelableLongOption(Field),
}

impl ParseError {
    /// Returns the field containing the option that caused this error.
    #[must_use]
    pub fn field(&self) -> &Field {
        match self {
            ParseError::UnknownShortOption(_, field)
            | ParseError::UnknownLongOption(field)
            | ParseError::AmbiguousLongOption(field)
            | ParseError::NonPortableLongOption(field)
            | ParseError::UncancelableShortOption(_, field)
            | ParseError::UncancelableLongOption(field) => field,
        }
    }

    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets =
            Snippet::with_primary_span(&self.field().origin, self.field().value.as_str().into());
        if let ParseError::NonPortableLongOption(_) = self {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            });
        }
        report
    }
}

impl<'a> From<&'a ParseError> for Report<'a> {
    #[inline]
    fn from(error: &'a ParseError) -> Self {
        error.to_report()
    }
}

/// Tries to parse the next field in `args`.
///
/// Returns `Ok(true)` if the next field contained a short option, in which case
/// the parsed field is consumed from the iterator.
fn try_parse_short<'a, I: Iterator<Item = Field>>(
    option_specs: &'a [OptionSpec<'a>],
    args: &mut Peekable<I>,
    option_occurrences: &mut Vec<OptionOccurrence<'a>>,
) -> Result<bool, ParseError> {
    let field = match args.peek() {
        Some(field) => field,
        None => return Ok(false),
    };
    let mut chars = field.value.chars();
    let negate = match chars.next() {
        Some('-') => false,
        Some('+') => true,
        _ => return Ok(false),
    };
    match chars.next() {
        Some('-') if !negate => return Ok(false),
        Some('+') if negate => return Ok(false),
        None => return Ok(false),
        _ => (),
    }

    let field = args.next().unwrap();
    for c in field.value.chars().skip(1) {
        let spec = match option_specs.iter().find(|spec| spec.short == c) {
            Some(spec) => spec,
            None => return Err(ParseError::UnknownShortOption(c, field)),
        };
        if negate && spec.attr.is_none() {
            return Err(ParseError::UncancelableShortOption(c, field));
        }
        option_occurrences.push(OptionOccurrence {
            spec,
            state: if negate { State::Off } else { State::On },
            location: field.origin.clone(),
        });
    }
    Ok(true)
}

/// Tries to parse and consume the next field in `args`.
fn try_parse_long<'a, I: Iterator<Item = Field>>(
    option_specs: &'a [OptionSpec<'a>],
    mode: Mode,
    args: &mut Peekable<I>,
) -> Result<Option<OptionOccurrence<'a>>, ParseError> {
    let field = match args.peek() {
        Some(field) => field,
        None => return Ok(None),
    };

    let (name, negate) = if let Some(name) = field.value.strip_prefix("--") {
        (name, false)
    } else if let Some(name) = field.value.strip_prefix("++") {
        (name, true)
    } else {
        return Ok(None);
    };

    let mut option_specs = option_specs
        .iter()
        .filter(|spec| spec.long.starts_with(name));
    let spec = option_specs.next();
    let spec2 = option_specs.next();
    let field = args.next().unwrap();
    match spec {
        None => Err(ParseError::UnknownLongOption(field)),
        Some(_spec) if spec2.is_some() => Err(ParseError::AmbiguousLongOption(field)),
        // The uncancelable check comes first: an option that cannot be canceled
        // cannot be canceled regardless of the `Portable` option, so reporting
        // non-portability here would suggest that turning the option off makes
        // the command work.
        Some(spec) if negate && spec.attr.is_none() => {
            Err(ParseError::UncancelableLongOption(field))
        }
        Some(_spec) if !mode.non_portable_option_names => {
            Err(ParseError::NonPortableLongOption(field))
        }
        Some(spec) => Ok(Some(OptionOccurrence {
            spec,
            state: if negate { State::Off } else { State::On },
            location: field.origin,
        })),
    }
}

/// Parses command line arguments.
///
/// The first argument is a list of option specifications that should be
/// recognized by the parser.
///
/// The second argument selects the syntax this parser accepts. This parser
/// honors only [`Mode::non_portable_option_names`], which governs whether long
/// options are accepted; while it is `false`, a long option is rejected with
/// [`ParseError::NonPortableLongOption`]. The other fields of [`Mode`] describe
/// syntax this parser cannot produce: no option of the typeset built-in family
/// takes an argument, and its operands are never numbers. A future version is
/// expected to honor `options_after_operands` as well.
///
/// The third argument is a list of command line arguments to be parsed.
///
/// Note that the mode does not cover the operand forms POSIX specifies for the
/// `export` and `readonly` built-ins. Those are checked by [`interpret`], which
/// takes the state of the `Portable` shell option separately.
///
/// Returns a pair of option occurrences and operands, which can be passed to
/// [`interpret`] to get a [`Command`].
pub fn parse<'a>(
    option_specs: &'a [OptionSpec<'a>],
    mode: Mode,
    args: Vec<Field>,
) -> Result<(Vec<OptionOccurrence<'a>>, Vec<Field>), ParseError> {
    let mut args = args.into_iter().peekable();
    let mut options = Vec::new();
    loop {
        if args.next_if(|arg| arg.value == "--").is_some() {
            break;
        }
        if try_parse_short(option_specs, &mut args, &mut options)? {
            continue;
        }
        if let Some(result) = try_parse_long(option_specs, mode, &mut args)? {
            options.push(result);
        } else {
            break; // TODO option after operand
        }
    }
    let operands = args.collect();
    Ok((options, operands))
}

/// Error in interpreting command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InterpretError<'a> {
    /// Short option that cannot be used with the `-f` option
    #[error("option {} is inapplicable for function", .clashing.spec)]
    OptionInapplicableForFunction {
        /// Occurrence of the option that conflicts with the `-f` option
        clashing: OptionOccurrence<'a>,
        /// Occurrence of the `-f` option
        function: OptionOccurrence<'a>,
    },

    /// No operand is given without the `-p` option while POSIX portability is
    /// required.
    #[error("missing operand")]
    MissingOperand,

    /// One or more operands are given with the `-p` option while POSIX
    /// portability is required.
    #[error("unexpected operand")]
    UnexpectedOperands {
        /// Location of the field containing the `-p` option
        print: Location,
        /// Operands that cannot be used with the `-p` option
        ///
        /// This vector is never empty.
        operands: Vec<Field>,
    },
}

impl InterpretError<'_> {
    /// Converts the error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();

        match self {
            Self::OptionInapplicableForFunction { clashing, function } => {
                report.snippets = Snippet::with_primary_span(
                    &clashing.location,
                    format!("the {} option ...", clashing.spec).into(),
                );
                add_span(
                    &function.location.code,
                    Span {
                        range: function.location.byte_range(),
                        role: SpanRole::Primary {
                            label: "... cannot be used for -f/--functions".into(),
                        },
                    },
                    &mut report.snippets,
                );
            }

            // There is no operand to annotate, so the report has no snippet of
            // its own. The built-in name is annotated by the caller.
            Self::MissingOperand => {}

            Self::UnexpectedOperands { print, operands } => {
                report.snippets = Snippet::with_primary_span(
                    &operands[0].origin,
                    format!("{}: unexpected operand", operands[0].value).into(),
                );
                add_span(
                    &print.code,
                    Span {
                        range: print.byte_range(),
                        role: SpanRole::Supplementary {
                            label: "the -p option expects no operands".into(),
                        },
                    },
                    &mut report.snippets,
                );
            }
        }

        if let Self::MissingOperand | Self::UnexpectedOperands { .. } = self {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            });
        }

        report
    }
}

impl<'a> From<&'a InterpretError<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a InterpretError) -> Self {
        error.to_report()
    }
}

/// Interprets options and operands into a command.
///
/// You can pass the result of [`parse`] to this function to get a command.
///
/// `portable` should be the state of the `Portable` shell option. While it is
/// `On`, the operands must follow the syntax POSIX specifies for the export and
/// readonly built-ins, which is either `name…` or `-p`; an invocation that has
/// neither, or that has both, is rejected with
/// [`InterpretError::MissingOperand`] or
/// [`InterpretError::UnexpectedOperands`].
///
/// If `options` contain an `OptionSpec` that is not contained in
/// [`ALL_OPTIONS`], this function will panic.
pub fn interpret(
    options: Vec<OptionOccurrence>,
    operands: Vec<Field>,
    portable: State,
) -> Result<Command, InterpretError> {
    let mut functions_option_index = None;
    let mut global_option_index = None;
    let mut print_option_index = None;
    let mut print = operands.is_empty();
    let mut attrs = Vec::new();
    for (index, option) in options.iter().enumerate() {
        match option.spec.short {
            'f' => functions_option_index = Some(index),
            'g' => global_option_index = Some(index),
            'p' => {
                print_option_index = Some(index);
                print = true;
            }
            'X' => attrs.push((index, Attr::Export, !option.state)),
            _ => attrs.push((index, option.spec.attr.unwrap(), option.state)),
        }
    }

    if portable == State::On {
        match print_option_index {
            None if operands.is_empty() => return Err(InterpretError::MissingOperand),

            Some(index) if !operands.is_empty() => {
                let print = { options }.swap_remove(index).location;
                return Err(InterpretError::UnexpectedOperands { print, operands });
            }

            _ => {}
        }
    }

    if let Some(functions_option_index) = functions_option_index {
        if let Some(global_option_index) = global_option_index {
            return Err(InterpretError::OptionInapplicableForFunction {
                clashing: options[global_option_index].clone(),
                function: options[functions_option_index].clone(),
            });
        }

        let functions = operands;
        let attrs = attrs
            .into_iter()
            .map(|(index, attr, state)| Ok((attr.try_into().or(Err(index))?, state)))
            .collect::<Result<Vec<(FunctionAttr, State)>, usize>>()
            .map_err(|attr_index| InterpretError::OptionInapplicableForFunction {
                clashing: options[attr_index].clone(),
                function: options[functions_option_index].clone(),
            })?;

        if print {
            Ok((PrintFunctions { functions, attrs }).into())
        } else {
            Ok((SetFunctions { functions, attrs }).into())
        }
    } else {
        let variables = operands;
        let attrs = attrs
            .into_iter()
            .map(|(_index, attr, state)| Ok((attr.try_into()?, state)))
            .collect::<Result<Vec<(VariableAttr, State)>, UnsupportedAttr>>()
            .expect("all attributes should be convertible to VariableAttr");
        let scope = match global_option_index {
            Some(_) => Scope::Global,
            None => Scope::Local,
        };

        if print {
            let pv = PrintVariables {
                variables,
                attrs,
                scope,
            };
            Ok(pv.into())
        } else {
            let sv = SetVariables {
                variables,
                attrs,
                scope,
            };
            Ok(sv.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn parse_empty_arguments() {
        let result = parse(&[], Mode::with_extensions(), vec![]).unwrap();
        assert_eq!(result, (vec![], vec![]));
    }

    #[test]
    fn parse_some_operands_without_options() {
        let vars = Field::dummies(["foo", "bar"]);
        let result = parse(&[], Mode::with_extensions(), vars.clone()).unwrap();
        assert_eq!(result, (vec![], vars));
    }

    #[test]
    fn parse_short_print_option_without_operands() {
        let result = parse(ALL_OPTIONS, Mode::with_extensions(), Field::dummies(["-p"])).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("-p"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_many_short_options() {
        let args = Field::dummies(["-p", "+xr"]);
        let result = parse(ALL_OPTIONS, Mode::with_extensions(), args.clone()).unwrap();
        assert_matches!(&result.0[..], [option1, option2, option3] => {
            assert_eq!(option1.spec, &PRINT_OPTION);
            assert_eq!(option1.state, State::On);
            assert_eq!(option1.location, Location::dummy("-p"));
            assert_eq!(option2.spec, &EXPORT_OPTION);
            assert_eq!(option2.state, State::Off);
            assert_eq!(option2.location, Location::dummy("+xr"));
            assert_eq!(option3.spec, &READONLY_OPTION);
            assert_eq!(option3.state, State::Off);
            assert_eq!(option3.location, Location::dummy("+xr"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_long_print_option_without_operands() {
        let result = parse(
            ALL_OPTIONS,
            Mode::with_extensions(),
            Field::dummies(["--print"]),
        )
        .unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("--print"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_print_option_with_operands() {
        let vars = Field::dummies(["foo", "var"]);
        let mut args = Field::dummies(["-p"]);
        args.extend(vars.iter().cloned());
        let result = parse(ALL_OPTIONS, Mode::with_extensions(), args).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("-p"));
        });
        assert_eq!(result.1, vars);
    }

    #[test]
    fn parse_abbreviated_long_option() {
        let result = parse(
            ALL_OPTIONS,
            Mode::with_extensions(),
            Field::dummies(["--pri"]),
        )
        .unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("--pri"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_negated_short_export_option() {
        let result = parse(ALL_OPTIONS, Mode::with_extensions(), Field::dummies(["+x"])).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &EXPORT_OPTION);
            assert_eq!(option.state, State::Off);
            assert_eq!(option.location, Location::dummy("+x"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_negated_long_export_option() {
        let result = parse(
            ALL_OPTIONS,
            Mode::with_extensions(),
            Field::dummies(["++export"]),
        )
        .unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &EXPORT_OPTION);
            assert_eq!(option.state, State::Off);
            assert_eq!(option.location, Location::dummy("++export"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_separator() {
        let args = Field::dummies(["-p", "--", "-x"]);
        let result = parse(ALL_OPTIONS, Mode::with_extensions(), args.clone()).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("-p"));
        });
        assert_eq!(result.1, Field::dummies(["-x"]));
    }

    #[test]
    fn parse_unknown_short_option() {
        assert_eq!(
            parse(&[], Mode::with_extensions(), Field::dummies(["-p"])),
            Err(ParseError::UnknownShortOption('p', Field::dummy("-p"))),
        );
    }

    #[test]
    fn parse_unknown_long_option() {
        assert_eq!(
            parse(&[], Mode::with_extensions(), Field::dummies(["--print"])),
            Err(ParseError::UnknownLongOption(Field::dummy("--print"))),
        );
    }

    #[test]
    fn parse_negated_short_print_option() {
        assert_eq!(
            parse(ALL_OPTIONS, Mode::with_extensions(), Field::dummies(["+p"])),
            Err(ParseError::UncancelableShortOption('p', Field::dummy("+p"))),
        );
    }

    #[test]
    fn parse_negated_long_print_option() {
        assert_eq!(
            parse(
                ALL_OPTIONS,
                Mode::with_extensions(),
                Field::dummies(["++print"]),
            ),
            Err(ParseError::UncancelableLongOption(Field::dummy("++print"))),
        );
    }

    #[test]
    fn parse_ambiguous_long_option() {
        pub const EXPAND_OPTION: OptionSpec<'static> = OptionSpec {
            short: 'x',
            long: "expand",
            attr: None,
        };
        assert_eq!(
            parse(
                &[EXPORT_OPTION, EXPAND_OPTION],
                Mode::with_extensions(),
                Field::dummies(["++exp"]),
            ),
            Err(ParseError::AmbiguousLongOption(Field::dummy("++exp"))),
        );
    }

    #[test]
    fn parse_long_option_rejected_under_portable() {
        assert_eq!(
            parse(ALL_OPTIONS, Mode::default(), Field::dummies(["--print"])),
            Err(ParseError::NonPortableLongOption(Field::dummy("--print"))),
        );
    }

    #[test]
    fn parse_abbreviated_long_option_rejected_under_portable() {
        assert_eq!(
            parse(ALL_OPTIONS, Mode::default(), Field::dummies(["--pri"])),
            Err(ParseError::NonPortableLongOption(Field::dummy("--pri"))),
        );
    }

    #[test]
    fn parse_negated_long_option_rejected_under_portable() {
        assert_eq!(
            parse(ALL_OPTIONS, Mode::default(), Field::dummies(["++export"])),
            Err(ParseError::NonPortableLongOption(Field::dummy("++export"))),
        );
    }

    #[test]
    fn parse_negated_uncancelable_long_option_under_portable() {
        assert_eq!(
            parse(ALL_OPTIONS, Mode::default(), Field::dummies(["++print"])),
            Err(ParseError::UncancelableLongOption(Field::dummy("++print"))),
        );
    }

    #[test]
    fn parse_unknown_long_option_under_portable() {
        assert_eq!(
            parse(ALL_OPTIONS, Mode::default(), Field::dummies(["--foo"])),
            Err(ParseError::UnknownLongOption(Field::dummy("--foo"))),
        );
    }

    #[test]
    fn parse_short_option_accepted_under_portable() {
        let result = parse(ALL_OPTIONS, Mode::default(), Field::dummies(["-p"])).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("-p"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_operand_starting_with_double_hyphen_under_portable() {
        let args = Field::dummies(["--", "--print"]);
        let result = parse(ALL_OPTIONS, Mode::default(), args).unwrap();
        assert_eq!(result.0, []);
        assert_eq!(result.1, Field::dummies(["--print"]));
    }

    #[test]
    fn non_portable_long_option_report_mentions_portable_option() {
        let error = ParseError::NonPortableLongOption(Field::dummy("--print"));
        let report = error.to_report();
        assert_matches!(&report.footnotes[..], [footnote] => {
            assert_eq!(footnote.r#type, FootnoteType::Note);
            assert!(
                footnote.label.contains("portable"),
                "unexpected footnote: {:?}",
                footnote.label,
            );
        });
    }

    #[test]
    fn interpret_empty_arguments() {
        let result = interpret(vec![], vec![], State::Off).unwrap();
        assert_matches!(result, Command::PrintVariables(pv) => {
            assert_eq!(pv.variables, []);
            assert_eq!(pv.attrs, []);
            assert_eq!(pv.scope, Scope::Local);
        });
    }

    #[test]
    fn interpret_some_operands_without_options() {
        let vars = Field::dummies(["foo", "bar"]);
        let result = interpret(vec![], vars.clone(), State::Off).unwrap();
        assert_matches!(result, Command::SetVariables(sv) => {
            assert_eq!(sv.variables, vars);
            assert_eq!(sv.attrs, []);
            assert_eq!(sv.scope, Scope::Local);
        });
    }

    fn dummy_option_occurrence<'a>(spec: &'a OptionSpec<'a>, state: State) -> OptionOccurrence<'a> {
        OptionOccurrence {
            spec,
            state,
            location: Location::dummy(""),
        }
    }

    #[test]
    fn interpret_functions_option_without_operands() {
        let result = interpret(
            vec![dummy_option_occurrence(&FUNCTIONS_OPTION, State::On)],
            vec![],
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintFunctions(pf)) => {
            assert_eq!(pf.functions, []);
            assert_eq!(pf.attrs, []);
        });
    }

    #[test]
    fn interpret_functions_option_with_operands() {
        let functions = Field::dummies(["foo", "bar"]);
        let result = interpret(
            vec![dummy_option_occurrence(&FUNCTIONS_OPTION, State::On)],
            functions.clone(),
            State::Off,
        );
        assert_matches!(result, Ok(Command::SetFunctions(sf)) => {
            assert_eq!(sf.functions, functions);
            assert_eq!(sf.attrs, []);
        });
    }

    #[test]
    fn interpret_global_option_without_operands() {
        let result = interpret(
            vec![dummy_option_occurrence(&GLOBAL_OPTION, State::On)],
            vec![],
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, []);
            assert_eq!(pv.attrs, []);
            assert_eq!(pv.scope, Scope::Global);
        });
    }

    #[test]
    fn interpret_global_option_with_operands() {
        let vars = Field::dummies(["foo", "var"]);
        let result = interpret(
            vec![dummy_option_occurrence(&GLOBAL_OPTION, State::On)],
            vars.clone(),
            State::Off,
        );
        assert_matches!(result, Ok(Command::SetVariables(sv)) => {
            assert_eq!(sv.variables, vars);
            assert_eq!(sv.attrs, []);
            assert_eq!(sv.scope, Scope::Global);
        });
    }

    #[test]
    fn interpret_print_option_without_operands() {
        let result = interpret(
            vec![dummy_option_occurrence(&PRINT_OPTION, State::On)],
            vec![],
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, []);
            assert_eq!(pv.attrs, []);
            assert_eq!(pv.scope, Scope::Local);
        });
    }

    #[test]
    fn interpret_print_option_with_operands() {
        let vars = Field::dummies(["foo", "var"]);
        let result = interpret(
            vec![dummy_option_occurrence(&PRINT_OPTION, State::On)],
            vars.clone(),
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, vars);
            assert_eq!(pv.attrs, []);
            assert_eq!(pv.scope, Scope::Local);
        });
    }

    #[test]
    fn interpret_negated_export_option_without_operands() {
        let result = interpret(
            vec![dummy_option_occurrence(&EXPORT_OPTION, State::Off)],
            vec![],
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, []);
            assert_eq!(pv.attrs, [(VariableAttr::Export, State::Off)]);
            assert_eq!(pv.scope, Scope::Local);
        });
    }

    #[test]
    fn interpret_negated_export_option_with_operands() {
        let vars = Field::dummies(["foo", "bar"]);
        let result = interpret(
            vec![dummy_option_occurrence(&EXPORT_OPTION, State::Off)],
            vars.clone(),
            State::Off,
        );
        assert_matches!(result, Ok(Command::SetVariables(sv)) => {
            assert_eq!(sv.variables, vars);
            assert_eq!(sv.attrs, [(VariableAttr::Export, State::Off)]);
            assert_eq!(sv.scope, Scope::Local);
        });
    }

    #[test]
    fn interpret_function_names_for_printing() {
        let functions = Field::dummies(["foo", "bar"]);
        let result = interpret(
            vec![
                dummy_option_occurrence(&FUNCTIONS_OPTION, State::On),
                dummy_option_occurrence(&PRINT_OPTION, State::On),
            ],
            functions.clone(),
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintFunctions(pf)) => {
            assert_eq!(pf.functions, functions);
            assert_eq!(pf.attrs, []);
        });
    }

    #[test]
    fn interpret_function_attributes_for_printing() {
        let result = interpret(
            vec![
                dummy_option_occurrence(&FUNCTIONS_OPTION, State::On),
                dummy_option_occurrence(&PRINT_OPTION, State::On),
                dummy_option_occurrence(&READONLY_OPTION, State::Off),
            ],
            vec![],
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintFunctions(pf)) => {
            assert_eq!(pf.functions, vec![]);
            assert_eq!(pf.attrs, [(FunctionAttr::ReadOnly, State::Off)]);
        });
    }

    #[test]
    fn interpret_function_attributes_for_setting() {
        let functions = Field::dummies(["func"]);
        let result = interpret(
            vec![
                dummy_option_occurrence(&FUNCTIONS_OPTION, State::On),
                dummy_option_occurrence(&READONLY_OPTION, State::On),
            ],
            functions.clone(),
            State::Off,
        );
        assert_matches!(result, Ok(Command::SetFunctions(sf)) => {
            assert_eq!(sf.functions, functions);
            assert_eq!(sf.attrs, [(FunctionAttr::ReadOnly, State::On)]);
        });
    }

    #[test]
    fn interpret_inapplicable_attribute_option_for_functions() {
        let f_option = dummy_option_occurrence(&FUNCTIONS_OPTION, State::On);
        let x_option = dummy_option_occurrence(&EXPORT_OPTION, State::On);
        let result = interpret(vec![f_option.clone(), x_option.clone()], vec![], State::Off);
        assert_eq!(
            result,
            Err(InterpretError::OptionInapplicableForFunction {
                clashing: x_option,
                function: f_option,
            }),
        );
    }

    #[test]
    fn interpret_global_option_with_functions_option() {
        let f_option = dummy_option_occurrence(&FUNCTIONS_OPTION, State::On);
        let g_option = dummy_option_occurrence(&GLOBAL_OPTION, State::On);
        let result = interpret(vec![f_option.clone(), g_option.clone()], vec![], State::Off);
        assert_eq!(
            result,
            Err(InterpretError::OptionInapplicableForFunction {
                clashing: g_option,
                function: f_option,
            }),
        );
    }

    #[test]
    fn interpret_portable_operands_without_print_option() {
        let result = interpret(vec![], vec![], State::On);
        assert_eq!(result, Err(InterpretError::MissingOperand));

        let operands = Field::dummies(["foo", "bar"]);
        let result = interpret(vec![], operands.clone(), State::On);
        assert_matches!(result, Ok(Command::SetVariables(sv)) => {
            assert_eq!(sv.variables, operands);
        });
    }

    #[test]
    fn interpret_portable_operands_with_print_option() {
        let print = dummy_option_occurrence(&PRINT_OPTION, State::On);
        let result = interpret(vec![print.clone()], vec![], State::On);
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, []);
        });

        let operands = Field::dummies(["foo", "bar"]);
        let result = interpret(vec![print.clone()], operands.clone(), State::On);
        assert_eq!(
            result,
            Err(InterpretError::UnexpectedOperands {
                print: print.location,
                operands,
            }),
        );
    }

    #[test]
    fn interpret_non_portable_operands_are_accepted_while_portable_is_off() {
        let result = interpret(vec![], vec![], State::Off);
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, []);
        });

        let print = dummy_option_occurrence(&PRINT_OPTION, State::On);
        let operands = Field::dummies(["foo"]);
        let result = interpret(vec![print], operands.clone(), State::Off);
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, operands);
        });
    }

    #[test]
    fn interpret_unexport_option_for_variables() {
        let result = interpret(
            vec![dummy_option_occurrence(&UNEXPORT_OPTION, State::On)],
            vec![],
            State::Off,
        );
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, vec![]);
            assert_eq!(pv.attrs, [(VariableAttr::Export, State::Off)]);
            assert_eq!(pv.scope, Scope::Local);
        });
    }
}
