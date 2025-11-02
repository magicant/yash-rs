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

//! Command line argument parser for the cd built-in

use super::Command;
use super::Mode;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use std::borrow::Cow;
use std::collections::VecDeque;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::Location;
#[allow(deprecated)]
use yash_env::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_env::source::pretty::{Report, ReportType, Snippet};

/// Error in parsing command line arguments
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// An error occurred in the common parser.
    #[error(transparent)]
    CommonError(#[from] crate::common::syntax::ParseError<'static>),

    /// The `-e` option is used without the `-P` option.
    ///
    /// The `Location` indicates the argument containing the `-e` option.
    #[error("-e option must be used with -P (and not -L)")]
    EnsurePwdNotPhysical(Location),

    /// The operand is an empty string.
    #[error("empty operand")]
    EmptyOperand(Field),

    /// More than one operand is given.
    ///
    /// The `Vec` contains the extra operands.
    #[error("unexpected operand")]
    UnexpectedOperands(Vec<Field>),
}

impl Error {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let (location, label) = match self {
            Self::CommonError(e) => return e.to_report(),
            Self::EnsurePwdNotPhysical(location) => {
                (location, "-e option must be used with -P".into())
            }
            Self::EmptyOperand(operand) => (&operand.origin, "empty operand".into()),
            Self::UnexpectedOperands(operands) => (
                &operands[0].origin,
                format!("{}: unexpected operand", operands[0].value).into(),
            ),
        };

        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = Snippet::with_primary_span(location, label);
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
        use Error::*;
        match self {
            CommonError(e) => e.main_annotation(),
            EnsurePwdNotPhysical(location) => {
                Annotation::new(AnnotationType::Error, "-e option".into(), location)
            }
            EmptyOperand(operand) => Annotation::new(
                AnnotationType::Error,
                "empty operand".into(),
                &operand.origin,
            ),
            UnexpectedOperands(operands) => Annotation::new(
                AnnotationType::Error,
                format!("{}: unexpected operand", operands[0].value).into(),
                &operands[0].origin,
            ),
        }
    }
}

/// Result of parsing command line arguments
pub type Result = std::result::Result<Command, Error>;

const OPTION_SPECS: &[OptionSpec] = &[
    OptionSpec::new().short('e').long("ensure-pwd"),
    OptionSpec::new().short('L').long("logical"),
    OptionSpec::new().short('P').long("physical"),
];

/// Parses command line arguments for the cd built-in.
pub fn parse(env: &Env, args: Vec<Field>) -> Result {
    let parser_mode = crate::common::syntax::Mode::with_env(env);
    let (options, operands) = parse_arguments(OPTION_SPECS, parser_mode, args)?;

    let mut ensure_pwd_option_location = None;
    let mut mode = Mode::default();
    for option in options {
        match option.spec.get_short() {
            Some('e') => ensure_pwd_option_location = Some(option.location),
            Some('L') => mode = Mode::Logical,
            Some('P') => mode = Mode::Physical,
            _ => unreachable!(),
        }
    }

    let ensure_pwd = match (ensure_pwd_option_location, mode) {
        (Some(_), Mode::Physical) => true,
        (Some(location), _) => return Err(Error::EnsurePwdNotPhysical(location)),
        (None, _) => false,
    };

    let mut operands = VecDeque::from(operands);
    let operand = operands.pop_front();
    if !operands.is_empty() {
        return Err(Error::UnexpectedOperands(operands.into()));
    }

    let operand = match operand {
        Some(operand) if operand.value.is_empty() => return Err(Error::EmptyOperand(operand)),
        operand => operand,
    };
    Ok(Command {
        mode,
        ensure_pwd,
        operand,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments() {
        let env = Env::new_virtual();
        let result = parse(&env, vec![]);
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Logical,
                ensure_pwd: false,
                operand: None,
            })
        );
    }

    #[test]
    fn logical_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-L"]));
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Logical,
                ensure_pwd: false,
                operand: None,
            })
        );
    }

    #[test]
    fn physical_option() {
        let env = Env::new_virtual();
        let result = parse(&env, Field::dummies(["-P"]));
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Physical,
                ensure_pwd: false,
                operand: None,
            })
        );
    }

    #[test]
    fn last_option_wins() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-L", "-P"]));
        assert_eq!(result.unwrap().mode, Mode::Physical);

        let result = parse(&env, Field::dummies(["-P", "-L"]));
        assert_eq!(result.unwrap().mode, Mode::Logical);

        let result = parse(&env, Field::dummies(["-L", "-P", "-L"]));
        assert_eq!(result.unwrap().mode, Mode::Logical);

        let result = parse(&env, Field::dummies(["-PLP"]));
        assert_eq!(result.unwrap().mode, Mode::Physical);
    }

    #[test]
    fn ensure_pwd_option_with_physical_option() {
        let env = Env::new_virtual();

        let result = parse(&env, Field::dummies(["-e", "-P"]));
        assert!(result.unwrap().ensure_pwd);

        let result = parse(&env, Field::dummies(["-P", "-e"]));
        assert!(result.unwrap().ensure_pwd);

        let result = parse(&env, Field::dummies(["-eLP"]));
        assert!(result.unwrap().ensure_pwd);
    }

    #[test]
    fn with_operand() {
        let env = Env::new_virtual();
        let operand = Field::dummy("foo/bar");
        let result = parse(&env, vec![operand.clone()]);
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::default(),
                ensure_pwd: false,
                operand: Some(operand),
            })
        );
    }

    #[test]
    fn option_and_operand() {
        let env = Env::new_virtual();
        let operand = Field::dummy("foo/bar");
        let args = vec![Field::dummy("-L"), Field::dummy("--"), operand.clone()];
        let result = parse(&env, args);
        assert_eq!(
            result,
            Ok(Command {
                mode: Mode::Logical,
                ensure_pwd: false,
                operand: Some(operand),
            })
        );
    }

    #[test]
    fn ensure_pwd_option_with_logical_option() {
        let env = Env::new_virtual();
        let e = Field::dummy("-e");

        let result = parse(&env, vec![Field::dummy("-L"), e.clone()]);
        assert_eq!(result, Err(Error::EnsurePwdNotPhysical(e.origin.clone())));

        let result = parse(&env, vec![e.clone()]);
        assert_eq!(result, Err(Error::EnsurePwdNotPhysical(e.origin)));
    }

    #[test]
    fn empty_operand() {
        let env = Env::new_virtual();
        let operand = Field::dummy("");
        let result = parse(&env, vec![operand.clone()]);
        assert_eq!(result, Err(Error::EmptyOperand(operand)));
    }

    #[test]
    fn unexpected_operand() {
        let env = Env::new_virtual();
        let operand1 = Field::dummy("foo");
        let operand2 = Field::dummy("bar");
        let result = parse(&env, vec![operand1, operand2.clone()]);
        assert_eq!(result, Err(Error::UnexpectedOperands(vec![operand2])));
    }

    #[test]
    fn unexpected_operands_after_options() {
        let env = Env::new_virtual();
        let args = Field::dummies(["-LP", "-L", "--", "one", "two", "three"]);
        let extra_operands = args[4..].to_vec();
        let result = parse(&env, args);
        assert_eq!(result, Err(Error::UnexpectedOperands(extra_operands)));
    }
}
