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
use crate::common::syntax::{Mode, OptionSpec, ParseError, parse_arguments};
use std::borrow::Cow;
use std::num::ParseIntError;
use std::str::FromStr;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::Location;
#[allow(deprecated)]
use yash_env::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_env::source::pretty::{Report, ReportType, Snippet, Span, SpanRole, add_span};
use yash_env::system::resource::Resource;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common syntax parser.
    #[error(transparent)]
    CommonError(#[from] ParseError<'static>),

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
        let snippets = match self {
            Self::CommonError(e) => return e.to_report(),
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
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for Error {
    fn message_title(&self) -> Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        match self {
            Self::CommonError(e) => e.main_annotation(),
            Self::AllWithOperand(operand) => Annotation::new(
                AnnotationType::Error,
                format!("{}: unexpected operand", operand.value).into(),
                &operand.origin,
            ),
            Self::ShowingBoth { soft, .. } => Annotation::new(
                AnnotationType::Error,
                "soft limit required here".into(),
                soft,
            ),
            Self::TooManyResources(location) => {
                Annotation::new(AnnotationType::Error, "unexpected option".into(), location)
            }
            Self::TooManyOperands(operands) => Annotation::new(
                AnnotationType::Error,
                format!("{}: unexpected operand", operands[1].value).into(),
                &operands[1].origin,
            ),
            Self::InvalidLimit(operand, e) => Annotation::new(
                AnnotationType::Error,
                format!("{}: invalid limit ({})", operand.value, e).into(),
                &operand.origin,
            ),
        }
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        if let Self::ShowingBoth { soft: _, hard } = self {
            results.extend(std::iter::once(Annotation::new(
                AnnotationType::Error,
                "hard limit required here".into(),
                hard,
            )));
        }
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
    OptionSpec::new().short('k').long("kqueues"),
    OptionSpec::new().short('x').long("locks"),
    OptionSpec::new().short('l').long("memlock"),
    OptionSpec::new().short('q').long("msgqueue"),
    OptionSpec::new().short('e').long("nice"),
    OptionSpec::new().short('n').long("nofile"),
    OptionSpec::new().short('u').long("nproc"),
    OptionSpec::new().short('m').long("rss"),
    OptionSpec::new().short('r').long("rtprio"),
    OptionSpec::new().short('R').long("rttime"),
    OptionSpec::new().short('b').long("sbsize"),
    OptionSpec::new().short('i').long("sigpending"),
    OptionSpec::new().short('s').long("stack"),
    OptionSpec::new().short('w').long("swap"),
];

/// Parses command line arguments.
pub fn parse(env: &Env, args: Vec<Field>) -> Result {
    let (options, operands) = parse_arguments(OPTION_SPECS, Mode::with_env(env), args)?;

    let mut resource_option = None;
    let mut hard = None;
    let mut soft = None;

    for option in options {
        match option.spec.get_short().unwrap() {
            'H' => hard = Some(option.location),
            'S' => soft = Some(option.location),
            c => {
                if resource_option.is_some_and(|c2| c2 != c) {
                    return Err(Error::TooManyResources(option.location));
                }
                resource_option = Some(c);
            }
        }
    }

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
