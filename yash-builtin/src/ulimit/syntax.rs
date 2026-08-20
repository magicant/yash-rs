// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Command-line argument parser for the `ulimit` built-in

use super::{Command, ResourceExt as _, SetLimitType, SetLimitValue, ShowLimitType};
use crate::common::syntax::{
    ConflictingOptionError, Mode, OptionOccurrence, OptionSpec, ParseError, parse_arguments,
};
use std::num::ParseIntError;
use std::str::FromStr;
use thiserror::Error;
use yash_env::Env;
use yash_env::option::Option::Portable;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::source::Location;
use yash_env::source::pretty::{
    Footnote, FootnoteType, Report, ReportType, Snippet, Span, SpanRole, add_span,
};
use yash_env::system::resource::Resource;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common syntax parser.
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),

    /// Option letters are grouped in a single argument while POSIX portability
    /// is required.
    ///
    /// The location is that of the argument containing the grouped options.
    #[error("cannot group option letters in a single argument")]
    GroupedOptions(Location),

    /// The `-H` and `-S` options are given together while POSIX portability is
    /// required.
    #[error(transparent)]
    ConflictingOption(#[from] ConflictingOptionError<'static>),

    /// An option other than `-H` and `-S` is repeated while POSIX portability
    /// is required.
    #[error("cannot specify the -{option} option more than once")]
    RepeatedOption {
        option: char,
        first: Location,
        second: Location,
    },

    /// The `-a` option is given with a resource limit operand.
    #[error("cannot set limit for -a")]
    AllWithOperand(Field),

    /// Both the `-H` and `-S` options are given without a resource limit
    /// operand.
    #[error("cannot show both hard and soft limits at once")]
    ShowingBoth { soft: Location, hard: Location },

    /// More than one resource is specified.
    #[error("cannot specify more than one resource")]
    TooManyResources(Location),

    /// More than one operand is given.
    ///
    /// The vector contains *all* the operands, including the first proper one.
    #[error("too many operands")]
    TooManyOperands(Vec<Field>),

    /// An operand is not a valid limit.
    #[error("invalid limit")]
    InvalidLimit(Field, ParseIntError),
}

impl Error {
    /// Converts the error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        /// Footnote attributing the error to the `portable` shell option
        fn portable_footnote() -> Footnote<'static> {
            Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            }
        }

        let snippets = match self {
            Self::CommonError(e) => return e.to_report(),
            Self::ConflictingOption(e) => {
                let mut report = e.to_report();
                report.footnotes.push(portable_footnote());
                return report;
            }
            Self::GroupedOptions(location) => Snippet::with_primary_span(
                location,
                "these options must be specified in separate arguments".into(),
            ),
            Self::RepeatedOption { first, second, .. } => {
                let mut snippets = Snippet::with_primary_span(first, "first specified here".into());
                add_span(
                    &second.code,
                    Span {
                        range: second.byte_range(),
                        role: SpanRole::Primary {
                            label: "specified again here".into(),
                        },
                    },
                    &mut snippets,
                );
                snippets
            }
            Self::AllWithOperand(field) => Snippet::with_primary_span(
                &field.origin,
                format!("{field}: unexpected operand").into(),
            ),
            Self::ShowingBoth { soft, hard } => {
                let mut snippets =
                    Snippet::with_primary_span(soft, "soft limit requested here".into());
                add_span(
                    &hard.code,
                    Span {
                        range: hard.byte_range(),
                        role: SpanRole::Primary {
                            label: "hard limit requested here".into(),
                        },
                    },
                    &mut snippets,
                );
                snippets
            }
            Self::TooManyResources(location) => {
                Snippet::with_primary_span(location, "unexpected option".into())
            }
            Self::TooManyOperands(fields) => Snippet::with_primary_span(
                &fields[1].origin,
                format!("{}: unexpected operand", fields[1].value).into(),
            ),
            Self::InvalidLimit(operand, parse_int_error) => Snippet::with_primary_span(
                &operand.origin,
                format!("{operand}: invalid limit ({parse_int_error})").into(),
            ),
        };
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = snippets;
        if let Self::GroupedOptions(_) | Self::RepeatedOption { .. } = self {
            report.footnotes.push(portable_footnote());
        }
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Result of parsing command line arguments
pub type Result = std::result::Result<Command, Error>;

/// Command-line options for the `ulimit` built-in
const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new().short('H').long("hard"),
    OptionSpec::new().short('S').long("soft"),
    OptionSpec::new().short('a').long("all"),
    OptionSpec::new().short('v').long("as"),
    OptionSpec::new().short('c').long("core"),
    OptionSpec::new().short('t').long("cpu"),
    OptionSpec::new().short('d').long("data"),
    OptionSpec::new().short('f').long("fsize"),
    OptionSpec::new().short('k').long("kqueues").extension(true),
    OptionSpec::new().short('x').long("locks").extension(true),
    OptionSpec::new().short('l').long("memlock").extension(true),
    OptionSpec::new()
        .short('q')
        .long("msgqueue")
        .extension(true),
    OptionSpec::new().short('e').long("nice").extension(true),
    OptionSpec::new().short('n').long("nofile"),
    OptionSpec::new().short('u').long("nproc").extension(true),
    OptionSpec::new().short('m').long("rss").extension(true),
    OptionSpec::new().short('r').long("rtprio").extension(true),
    OptionSpec::new().short('R').long("rttime").extension(true),
    OptionSpec::new().short('b').long("sbsize").extension(true),
    OptionSpec::new()
        .short('i')
        .long("sigpending")
        .extension(true),
    OptionSpec::new().short('s').long("stack"),
    OptionSpec::new().short('w').long("swap").extension(true),
];

/// Rejects the option syntax POSIX does not guarantee for the `ulimit` utility.
///
/// This function checks the grouping and repetition of options. The `-H` and
/// `-S` options being used together is not checked here because reporting it
/// consumes the option occurrences.
fn check_option_syntax(options: &[OptionOccurrence]) -> std::result::Result<(), Error> {
    // POSIX exempts the ulimit utility from Utility Syntax Guideline 5, so an
    // implementation need not recognize grouped option letters like `-fH`.
    if let Some(option) = options.iter().find(|o| o.spelling.is_grouped()) {
        return Err(Error::GroupedOptions(option.location.clone()));
    }

    // POSIX leaves the behavior unspecified if an option other than -H and -S
    // is repeated.
    for (index, option) in options.iter().enumerate() {
        let short = option.spec.get_short().unwrap();
        if matches!(short, 'H' | 'S') {
            continue;
        }
        let first = options[..index]
            .iter()
            .find(|o| o.spec.get_short() == Some(short));
        if let Some(first) = first {
            return Err(Error::RepeatedOption {
                option: short,
                first: first.location.clone(),
                second: option.location.clone(),
            });
        }
    }

    Ok(())
}

/// Parses command line arguments.
///
/// While the `Portable` shell option is on, this function additionally rejects
/// the option syntax POSIX does not guarantee. POSIX exempts the `ulimit`
/// utility from Utility Syntax Guideline 5, so option letters grouped in a
/// single argument (as in `ulimit -fH`) are rejected with
/// [`Error::GroupedOptions`]. POSIX writes the synopsis as
/// `ulimit [-H|-S] ...`, so the two options cannot be used together, which is
/// rejected with [`Error::ConflictingOption`]. POSIX also leaves the behavior
/// unspecified if an option other than `-H` and `-S` is repeated, which is
/// rejected with [`Error::RepeatedOption`]; repeating `-H` or `-S` remains
/// valid.
pub fn parse<S>(env: &Env<S>, args: Vec<Field>) -> Result {
    let (options, operands) = parse_arguments(OPTION_SPECS, Mode::with_env(env), args)?;
    let portable = env.options.get(Portable) == State::On;

    if portable {
        check_option_syntax(&options)?;
    }

    let mut resource_option = None;
    let mut hard = None;
    let mut soft = None;

    for option in options {
        match option.spec.get_short().unwrap() {
            'H' => hard = Some(option),
            'S' => soft = Some(option),
            c => {
                if resource_option.is_some_and(|c2| c2 != c) {
                    return Err(Error::TooManyResources(option.location));
                }
                resource_option = Some(c);
            }
        }
    }

    // POSIX writes the synopsis as `ulimit [-H|-S] ...`, so only one of the
    // two options may be used. Repeating the same one is not a conflict.
    if portable && let (Some(hard), Some(soft)) = (&hard, &soft) {
        let conflict = vec![hard.clone(), soft.clone()];
        return Err(ConflictingOptionError::new(conflict).into());
    }

    let hard = hard.map(|option| option.location);
    let soft = soft.map(|option| option.location);

    let resource = match resource_option {
        Some('a') => {
            return if let Some(operand) = operands.into_iter().next() {
                Err(Error::AllWithOperand(operand))
            } else {
                Ok(Command::ShowAll(show_limit_type(hard, soft)?))
            };
        }

        Some(option_char) => Resource::ALL
            .iter()
            .copied()
            .find(|r| r.option() == option_char)
            .unwrap(),

        None => Resource::FSIZE,
    };

    if operands.len() > 1 {
        return Err(Error::TooManyOperands(operands));
    }

    if let Some(operand) = { operands }.pop() {
        let limit_type = set_limit_type(hard, soft);
        let value = parse_value(operand)?;
        return Ok(Command::Set(resource, limit_type, value));
    }

    Ok(Command::ShowOne(resource, show_limit_type(hard, soft)?))
}

fn show_limit_type(
    hard: Option<Location>,
    soft: Option<Location>,
) -> std::result::Result<ShowLimitType, Error> {
    match (hard, soft) {
        (None, _) => Ok(ShowLimitType::Soft),
        (Some(_), None) => Ok(ShowLimitType::Hard),
        (Some(hard), Some(soft)) => Err(Error::ShowingBoth { soft, hard }),
    }
}

fn set_limit_type(hard: Option<Location>, soft: Option<Location>) -> SetLimitType {
    match (hard, soft) {
        (None, Some(_)) => SetLimitType::Soft,
        (Some(_), None) => SetLimitType::Hard,
        (None, None) | (Some(_), Some(_)) => SetLimitType::Both,
    }
}

fn parse_value(operand: Field) -> std::result::Result<SetLimitValue, Error> {
    operand
        .value
        .parse()
        .map_err(|e| Error::InvalidLimit(operand, e))
}

impl FromStr for SetLimitValue {
    type Err = ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "unlimited" => Ok(Self::Unlimited),
            "soft" => Ok(Self::CurrentSoft),
            "hard" => Ok(Self::CurrentHard),
            _ => Ok(Self::Number(s.parse()?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn show_default_soft_default_fsize() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(
            result,
            Ok(Command::ShowOne(Resource::FSIZE, ShowLimitType::Soft))
        );
    }

    #[test]
    fn show_explicit_soft_default_fsize() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-S"]));
        assert_eq!(
            result,
            Ok(Command::ShowOne(Resource::FSIZE, ShowLimitType::Soft))
        );
    }

    #[test]
    fn show_explicit_hard_default_fsize() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-H"]));
        assert_eq!(
            result,
            Ok(Command::ShowOne(Resource::FSIZE, ShowLimitType::Hard))
        );
    }

    #[test]
    fn show_cpu_default_soft() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-t"]));
        assert_eq!(
            result,
            Ok(Command::ShowOne(Resource::CPU, ShowLimitType::Soft))
        );
    }

    #[test]
    fn show_cpu_explicit_hard() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-t", "-H"]));
        assert_eq!(
            result,
            Ok(Command::ShowOne(Resource::CPU, ShowLimitType::Hard))
        );
    }

    #[test]
    fn show_all_default_soft() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-a"]));
        assert_eq!(result, Ok(Command::ShowAll(ShowLimitType::Soft)));
    }

    #[test]
    fn show_all_explicit_soft() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-Sa"]));
        assert_eq!(result, Ok(Command::ShowAll(ShowLimitType::Soft)));
    }

    #[test]
    fn show_all_explicit_hard() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-aH"]));
        assert_eq!(result, Ok(Command::ShowAll(ShowLimitType::Hard)));
    }

    #[test]
    fn set_default_both_default_fsize() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Both,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn set_explicit_soft_default_fsize() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-S", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Soft,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn set_explicit_hard_default_fsize() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-H", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Hard,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn set_default_both_explicit_data() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-d", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::DATA,
                SetLimitType::Both,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn set_explicit_soft_explicit_data() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-Sd", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::DATA,
                SetLimitType::Soft,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn set_explicit_hard_explicit_data() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-Hd", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::DATA,
                SetLimitType::Hard,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn set_unlimited() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["unlimited"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Both,
                SetLimitValue::Unlimited
            ))
        );
    }

    #[test]
    fn set_all() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-a", "0"]));
        assert_eq!(result, Err(Error::AllWithOperand(Field::dummy("0"))));
    }

    #[test]
    fn show_hard_and_soft() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-H", "-S"]));
        assert_eq!(
            result,
            Err(Error::ShowingBoth {
                soft: Location::dummy("-S"),
                hard: Location::dummy("-H")
            })
        );
    }

    #[test]
    fn set_hard_and_soft() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-HS", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Both,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn redundant_limit_type_options() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-H", "-H", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Hard,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn more_than_one_resource() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-d", "-f"]));
        assert_eq!(result, Err(Error::TooManyResources(Location::dummy("-f"))));
    }

    #[test]
    fn redundant_resource_options() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-dd", "-d", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::DATA,
                SetLimitType::Both,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn too_many_operands() {
        let env = Env::new_virtual();
        let args = Field::dummies(["0", "1"]);
        let result = parse(&env, args.clone());
        assert_eq!(result, Err(Error::TooManyOperands(args)));
    }

    #[test]
    fn separate_options_accepted_under_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-S", "-f", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Soft,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn grouped_options_rejected_under_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-Sf", "0"]));
        assert_eq!(result, Err(Error::GroupedOptions(Location::dummy("-Sf"))));
    }

    #[test]
    fn set_hard_and_soft_rejected_under_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-H", "-S", "0"]));
        assert_matches!(result, Err(Error::ConflictingOption(_)));
    }

    #[test]
    fn repeated_limit_type_option_accepted_under_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-H", "-H", "0"]));
        assert_eq!(
            result,
            Ok(Command::Set(
                Resource::FSIZE,
                SetLimitType::Hard,
                SetLimitValue::Number(0)
            ))
        );
    }

    #[test]
    fn repeated_resource_option_rejected_under_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, State::On);
        let result = parse(&env, Field::dummies(["-d", "-d", "0"]));
        assert_eq!(
            result,
            Err(Error::RepeatedOption {
                option: 'd',
                first: Location::dummy("-d"),
                second: Location::dummy("-d"),
            })
        );
    }

    #[test]
    fn set_limit_value_from_str_number() {
        assert_eq!("0".parse(), Ok(SetLimitValue::Number(0)));
        assert_eq!("1".parse(), Ok(SetLimitValue::Number(1)));
        assert_eq!("100".parse(), Ok(SetLimitValue::Number(100)));
    }

    #[test]
    fn set_limit_value_from_str_unlimited() {
        assert_eq!("unlimited".parse(), Ok(SetLimitValue::Unlimited));
    }

    #[test]
    fn set_limit_value_from_str_soft() {
        assert_eq!("soft".parse(), Ok(SetLimitValue::CurrentSoft));
    }

    #[test]
    fn set_limit_value_from_str_hard() {
        assert_eq!("hard".parse(), Ok(SetLimitValue::CurrentHard));
    }
}
