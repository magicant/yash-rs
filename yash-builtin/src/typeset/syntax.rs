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
use std::borrow::Cow;
use std::iter::Peekable;
use thiserror::Error;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_syntax::source::Location;
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};

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
            | ParseError::UncancelableShortOption(_, field)
            | ParseError::UncancelableLongOption(field) => field,
        }
    }
}

impl MessageBase for ParseError {
    fn message_title(&self) -> Cow<'_, str> {
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
        Some(spec) if negate && spec.attr.is_none() => {
            Err(ParseError::UncancelableLongOption(field))
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
/// recognized by the parser. The second argument is a list of command line
/// arguments to be parsed.
///
/// Returns a pair of option occurrences and operands, which can be passed to
/// [`interpret`] to get a [`Command`].
pub fn parse<'a>(
    option_specs: &'a [OptionSpec<'a>],
    // TODO: mode: Mode, (disabling long options & options after operands)
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
        if let Some(result) = try_parse_long(option_specs, &mut args)? {
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
}

impl MessageBase for InterpretError<'_> {
    fn message_title(&self) -> Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        match self {
            InterpretError::OptionInapplicableForFunction { clashing, .. } => Annotation::new(
                AnnotationType::Error,
                format!("the {} option ...", clashing.spec).into(),
                &clashing.location,
            ),
        }
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        match self {
            InterpretError::OptionInapplicableForFunction { function, .. } => {
                results.extend([Annotation::new(
                    AnnotationType::Error,
                    "... cannot be used for -f/--functions".into(),
                    &function.location,
                )])
            }
        }
    }
}

/// Interprets options and operands into a command.
///
/// You can pass the result of [`parse`] to this function to get a command.
///
/// If `options` contain an `OptionSpec` that is not contained in
/// [`ALL_OPTIONS`], this function will panic.
pub fn interpret(
    options: Vec<OptionOccurrence>,
    operands: Vec<Field>,
) -> Result<Command, InterpretError> {
    let mut functions_option_index = None;
    let mut global_option_index = None;
    let mut print = operands.is_empty();
    let mut attrs = Vec::new();
    for (index, option) in options.iter().enumerate() {
        match option.spec.short {
            'f' => functions_option_index = Some(index),
            'g' => global_option_index = Some(index),
            'p' => print = true,
            'X' => attrs.push((index, Attr::Export, !option.state)),
            _ => attrs.push((index, option.spec.attr.unwrap(), option.state)),
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
        let result = parse(&[], vec![]).unwrap();
        assert_eq!(result, (vec![], vec![]));
    }

    #[test]
    fn parse_some_operands_without_options() {
        let vars = Field::dummies(["foo", "bar"]);
        let result = parse(&[], vars.clone()).unwrap();
        assert_eq!(result, (vec![], vars));
    }

    #[test]
    fn parse_short_print_option_without_operands() {
        let result = parse(ALL_OPTIONS, Field::dummies(["-p"])).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("-p"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_long_print_option_without_operands() {
        let result = parse(ALL_OPTIONS, Field::dummies(["--print"])).unwrap();
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
        let result = parse(ALL_OPTIONS, args).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("-p"));
        });
        assert_eq!(result.1, vars);
    }

    #[test]
    fn parse_abbreviated_long_option() {
        let result = parse(ALL_OPTIONS, Field::dummies(["--pri"])).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &PRINT_OPTION);
            assert_eq!(option.state, State::On);
            assert_eq!(option.location, Location::dummy("--pri"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_negated_short_export_option() {
        let result = parse(ALL_OPTIONS, Field::dummies(["+x"])).unwrap();
        assert_matches!(&result.0[..], [option] => {
            assert_eq!(option.spec, &EXPORT_OPTION);
            assert_eq!(option.state, State::Off);
            assert_eq!(option.location, Location::dummy("+x"));
        });
        assert_eq!(result.1, []);
    }

    #[test]
    fn parse_negated_long_export_option() {
        let result = parse(ALL_OPTIONS, Field::dummies(["++export"])).unwrap();
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
        let result = parse(ALL_OPTIONS, args.clone()).unwrap();
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
            parse(&[], Field::dummies(["-p"])),
            Err(ParseError::UnknownShortOption('p', Field::dummy("-p"))),
        );
    }

    #[test]
    fn parse_unknown_long_option() {
        assert_eq!(
            parse(&[], Field::dummies(["--print"])),
            Err(ParseError::UnknownLongOption(Field::dummy("--print"))),
        );
    }

    #[test]
    fn parse_negated_short_print_option() {
        assert_eq!(
            parse(ALL_OPTIONS, Field::dummies(["+p"])),
            Err(ParseError::UncancelableShortOption('p', Field::dummy("+p"))),
        );
    }

    #[test]
    fn parse_negated_long_print_option() {
        assert_eq!(
            parse(ALL_OPTIONS, Field::dummies(["++print"])),
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
            parse(&[EXPORT_OPTION, EXPAND_OPTION], Field::dummies(["++exp"])),
            Err(ParseError::AmbiguousLongOption(Field::dummy("++exp"))),
        );
    }

    #[test]
    fn interpret_empty_arguments() {
        let result = interpret(vec![], vec![]).unwrap();
        assert_matches!(result, Command::PrintVariables(pv) => {
            assert_eq!(pv.variables, []);
            assert_eq!(pv.attrs, []);
            assert_eq!(pv.scope, Scope::Local);
        });
    }

    #[test]
    fn interpret_some_operands_without_options() {
        let vars = Field::dummies(["foo", "bar"]);
        let result = interpret(vec![], vars.clone()).unwrap();
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
        let result = interpret(vec![f_option.clone(), x_option.clone()], vec![]);
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
        let result = interpret(vec![f_option.clone(), g_option.clone()], vec![]);
        assert_eq!(
            result,
            Err(InterpretError::OptionInapplicableForFunction {
                clashing: g_option,
                function: f_option,
            }),
        );
    }

    #[test]
    fn interpret_unexport_option_for_variables() {
        let result = interpret(
            vec![dummy_option_occurrence(&UNEXPORT_OPTION, State::On)],
            vec![],
        );
        assert_matches!(result, Ok(Command::PrintVariables(pv)) => {
            assert_eq!(pv.variables, vec![]);
            assert_eq!(pv.attrs, [(VariableAttr::Export, State::Off)]);
            assert_eq!(pv.scope, Scope::Local);
        });
    }
}
