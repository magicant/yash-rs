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

//! Arithmetic expansion

use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::super::phrase::Phrase;
use super::super::ErrorCause;
use super::Env;
use super::Error;
use crate::expansion::expand_text;
use std::ops::Range;
use std::rc::Rc;
use yash_arith::eval;
use yash_env::option::Option::Unset;
use yash_env::option::State::{Off, On};
use yash_env::variable::ReadOnlyError;
use yash_env::variable::Scope::Global;
use yash_env::variable::Value::Scalar;
use yash_env::variable::Variable;
use yash_syntax::source::Code;
use yash_syntax::source::Location;
use yash_syntax::source::Source;
use yash_syntax::syntax::Text;

/// Types of errors that may occur in arithmetic expansion
///
/// This enum is essentially equivalent to `yash_arith::ErrorCause`. The
/// differences between the two are:
///
/// - `ArithError` defines all error variants flatly while `ErrorCause` has
///   nested variants.
/// - `ArithError` may contain informative [`Location`] that can be used to
///   produce an error message with annotated code while `ErrorCause` may just
///   specify a location as an index range.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ArithError {
    /// A value token contains an invalid character.
    #[error("invalid numeric constant")]
    InvalidNumericConstant,

    /// An expression contains a character that is not a whitespace, number, or
    /// number.
    #[error("invalid character")]
    InvalidCharacter,

    /// Expression with a missing value
    #[error("incomplete expression")]
    IncompleteExpression,

    /// Operator missing
    #[error("expected an operator")]
    MissingOperator,

    /// `(` without `)`
    #[error("unmatched parenthesis")]
    UnclosedParenthesis { opening_location: Location },

    /// `?` without `:`
    #[error("`?` without matching `:`")]
    QuestionWithoutColon { question_location: Location },

    /// `:` without `?`
    #[error("`:` without matching `?`")]
    ColonWithoutQuestion,

    /// Other error in operator usage
    #[error("invalid use of operator")]
    InvalidOperator,

    /// A variable value that is not a valid number
    #[error("invalid variable value: {0:?}")]
    InvalidVariableValue(String),

    /// Result out of bounds
    #[error("overflow")]
    Overflow,

    /// Division by zero
    #[error("division by zero")]
    DivisionByZero,

    /// Left bit-shifting of a negative value
    #[error("left-shifting a negative integer")]
    LeftShiftingNegative,

    /// Bit-shifting with a negative right-hand-side operand
    #[error("negative shift width")]
    ReverseShifting,

    /// Assignment with a left-hand-side operand not being a variable
    #[error("assignment to a non-variable")]
    AssignmentToValue,
}

impl ArithError {
    /// Returns a location related with this error and a message describing the
    /// location.
    #[must_use]
    pub fn related_location(&self) -> Option<(&Location, &'static str)> {
        use ArithError::*;
        match self {
            InvalidNumericConstant
            | InvalidCharacter
            | IncompleteExpression
            | MissingOperator
            | ColonWithoutQuestion
            | InvalidOperator
            | InvalidVariableValue(_)
            | Overflow
            | DivisionByZero
            | LeftShiftingNegative
            | ReverseShifting
            | AssignmentToValue => None,
            UnclosedParenthesis { opening_location } => {
                Some((opening_location, "the opening parenthesis was here"))
            }
            QuestionWithoutColon { question_location } => Some((question_location, "`?` was here")),
        }
    }
}

/// Error expanding an unset variable
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnsetVariable;

/// Converts `yash_arith::ErrorCause` into `initial::ErrorCause`.
///
/// The `source` argument must be the arithmetic expression being expanded.
/// It is used to reproduce a location contained in the error cause.
#[must_use]
fn convert_error_cause(
    cause: yash_arith::ErrorCause<UnsetVariable, ReadOnlyError>,
    source: &Rc<Code>,
) -> ErrorCause {
    use ArithError::*;
    match cause {
        yash_arith::ErrorCause::SyntaxError(e) => match e {
            yash_arith::SyntaxError::TokenError(e) => match e {
                yash_arith::TokenError::InvalidNumericConstant => {
                    ErrorCause::ArithError(InvalidNumericConstant)
                }
                yash_arith::TokenError::InvalidCharacter => {
                    ErrorCause::ArithError(InvalidCharacter)
                }
            },
            yash_arith::SyntaxError::IncompleteExpression => {
                ErrorCause::ArithError(IncompleteExpression)
            }
            yash_arith::SyntaxError::MissingOperator => ErrorCause::ArithError(MissingOperator),
            yash_arith::SyntaxError::UnclosedParenthesis { opening_location } => {
                let opening_location = Location {
                    code: Rc::clone(source),
                    range: opening_location,
                };
                ErrorCause::ArithError(UnclosedParenthesis { opening_location })
            }
            yash_arith::SyntaxError::QuestionWithoutColon { question_location } => {
                let question_location = Location {
                    code: Rc::clone(source),
                    range: question_location,
                };
                ErrorCause::ArithError(QuestionWithoutColon { question_location })
            }
            yash_arith::SyntaxError::ColonWithoutQuestion => {
                ErrorCause::ArithError(ColonWithoutQuestion)
            }
            yash_arith::SyntaxError::InvalidOperator => ErrorCause::ArithError(InvalidOperator),
        },
        yash_arith::ErrorCause::EvalError(e) => match e {
            yash_arith::EvalError::InvalidVariableValue(value) => {
                ErrorCause::ArithError(InvalidVariableValue(value))
            }
            yash_arith::EvalError::Overflow => ErrorCause::ArithError(Overflow),
            yash_arith::EvalError::DivisionByZero => ErrorCause::ArithError(DivisionByZero),
            yash_arith::EvalError::LeftShiftingNegative => {
                ErrorCause::ArithError(LeftShiftingNegative)
            }
            yash_arith::EvalError::ReverseShifting => ErrorCause::ArithError(ReverseShifting),
            yash_arith::EvalError::AssignmentToValue => ErrorCause::ArithError(AssignmentToValue),
            yash_arith::EvalError::GetVariableError(UnsetVariable) => ErrorCause::UnsetParameter,
            yash_arith::EvalError::AssignVariableError(e) => ErrorCause::AssignReadOnly(e),
        },
    }
}

struct VarEnv<'a> {
    env: &'a mut yash_env::Env,
    expression: &'a str,
    expansion_location: &'a Location,
}

impl<'a> yash_arith::Env for VarEnv<'a> {
    type GetVariableError = UnsetVariable;
    type AssignVariableError = ReadOnlyError;

    #[rustfmt::skip]
    fn get_variable(&self, name: &str) -> Result<Option<&str>, UnsetVariable> {
        if let Some(Variable { value: Some(Scalar(value)), .. }) = self.env.variables.get(name) {
            Ok(Some(value))
        } else {
            match self.env.options.get(Unset) {
                // TODO If the variable exists but is not scalar, UnsetVariable
                // does not seem to be the right error.
                Off => Err(UnsetVariable),
                On => Ok(None),
            }
        }
    }

    fn assign_variable(
        &mut self,
        name: &str,
        value: String,
        range: Range<usize>,
    ) -> Result<(), ReadOnlyError> {
        let code = Rc::new(Code {
            value: self.expression.to_string().into(),
            start_line_number: 1.try_into().unwrap(),
            source: Source::Arith {
                original: self.expansion_location.clone(),
            },
        });
        let location = Location { code, range };
        let value = Variable::new(value).set_assigned_location(location);
        self.env
            .assign_variable(Global, name.to_owned(), value)
            .map(drop)
    }
}

pub async fn expand(text: &Text, location: &Location, env: &mut Env<'_>) -> Result<Phrase, Error> {
    let (expression, exit_status) = expand_text(env.inner, text).await?;
    if exit_status.is_some() {
        env.last_command_subst_exit_status = exit_status;
    }

    let result = eval(
        &expression,
        &mut VarEnv {
            env: env.inner,
            expression: &expression,
            expansion_location: location,
        },
    );

    match result {
        Ok(value) => {
            let value = value.to_string();
            let chars = value
                .chars()
                .map(|c| AttrChar {
                    value: c,
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                })
                .collect();
            Ok(Phrase::Field(chars))
        }
        Err(error) => {
            let code = Rc::new(Code {
                value: expression.into(),
                start_line_number: 1.try_into().unwrap(),
                source: Source::Arith {
                    original: location.clone(),
                },
            });
            let cause = convert_error_cause(error.cause, &code);
            Err(Error {
                cause,
                location: Location {
                    code,
                    range: error.location,
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::Errno;
    use yash_env::variable::Scope::Global;

    #[test]
    fn var_env_get_variable_success() {
        use yash_arith::Env;
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(Global, "v".to_string(), Variable::new("value"))
            .unwrap();
        let location = Location::dummy("my location");
        let env = VarEnv {
            env: &mut env,
            expression: "v",
            expansion_location: &location,
        };

        let result = env.get_variable("v");
        assert_eq!(result, Ok(Some("value")));
    }

    #[test]
    fn var_env_get_variable_unset() {
        use yash_arith::Env;
        let mut env = yash_env::Env::new_virtual();
        let location = Location::dummy("my location");
        let env = VarEnv {
            env: &mut env,
            expression: "v",
            expansion_location: &location,
        };

        let result = env.get_variable("v");
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn var_env_get_variable_nounset() {
        use yash_arith::Env;
        let mut env = yash_env::Env::new_virtual();
        env.options.set(Unset, Off);
        let location = Location::dummy("my location");
        let env = VarEnv {
            env: &mut env,
            expression: "v",
            expansion_location: &location,
        };

        let result = env.get_variable("v");
        assert_eq!(result, Err(UnsetVariable));
    }

    #[test]
    fn successful_inner_text_expansion() {
        let text = "17%9".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(&text, &location, &mut env).now_or_never().unwrap();
        let c = AttrChar {
            value: '8',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
        assert_eq!(env.last_command_subst_exit_status, None);
    }

    #[test]
    fn non_zero_exit_status_from_inner_text_expansion() {
        in_virtual_system(|mut env, _state| async move {
            let text = "$(echo 0; return -n 63)".parse().unwrap();
            let location = Location::dummy("my location");
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let mut env = Env::new(&mut env);
            let result = expand(&text, &location, &mut env).await;
            let c = AttrChar {
                value: '0',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            };
            assert_eq!(result, Ok(Phrase::Char(c)));
            assert_eq!(env.last_command_subst_exit_status, Some(ExitStatus(63)));
        })
    }

    #[test]
    fn exit_status_is_kept_if_inner_text_expansion_contains_no_command_substitution() {
        let text = "0".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        env.last_command_subst_exit_status = Some(ExitStatus(123));
        let _ = expand(&text, &location, &mut env).now_or_never().unwrap();
        assert_eq!(env.last_command_subst_exit_status, Some(ExitStatus(123)));
    }

    #[test]
    fn error_in_inner_text_expansion() {
        let text = "$(x)".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(&text, &location, &mut env).now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(e.cause, ErrorCause::CommandSubstError(Errno::ENOSYS));
        assert_eq!(*e.location.code.value.borrow(), "$(x)");
        assert_eq!(e.location.range, 0..4);
    }

    #[test]
    fn variable_assigned_during_arithmetic_evaluation() {
        let text = "3 + (x = 4 * 6)".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env2 = Env::new(&mut env);
        let _ = expand(&text, &location, &mut env2).now_or_never().unwrap();

        let v = env.variables.get("x").unwrap();
        assert_eq!(v.value, Some(Scalar("24".to_string())));
        let location2 = v.last_assigned_location.as_ref().unwrap();
        assert_eq!(*location2.code.value.borrow(), "3 + (x = 4 * 6)");
        assert_eq!(location2.code.start_line_number.get(), 1);
        assert_eq!(location2.code.source, Source::Arith { original: location });
        assert_eq!(location2.range, 5..6);
        assert!(!v.is_exported);
        assert_eq!(v.read_only_location, None);
    }

    #[test]
    fn error_in_arithmetic_evaluation() {
        let text = "09".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(&text, &location, &mut env).now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::ArithError(ArithError::InvalidNumericConstant)
        );
        assert_eq!(*e.location.code.value.borrow(), "09");
        assert_eq!(e.location.code.source, Source::Arith { original: location });
        assert_eq!(e.location.range, 0..2);
    }
}
