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

use super::super::ErrorCause;
use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::super::phrase::Phrase;
use super::Env;
use super::Error;
use crate::Runtime;
use crate::expansion::AssignReadOnlyError;
use crate::expansion::expand_text;
use std::ops::Range;
use std::rc::Rc;
use yash_arith::Config;
use yash_arith::eval_with_config;
use yash_env::option::Option::{Portable, Unset};
use yash_env::option::State::{Off, On};
use yash_env::variable::Scope::Global;
use yash_syntax::source::Code;
use yash_syntax::source::Location;
use yash_syntax::source::Source;
use yash_syntax::syntax::Param;
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
#[non_exhaustive]
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

    /// An increment or decrement operator is used while the `portable` option
    /// is on.
    #[error("the increment and decrement operators are not portable")]
    NonPortableIncrementDecrement,

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

    /// An arithmetic error not recognized by this crate
    ///
    /// This variant is used to wrap an error cause that is returned by future
    /// versions of `yash_arith` but not yet recognized by this crate. The
    /// associated string is the `Display` representation of the error cause.
    #[error("{0}")]
    Unrecognized(String),
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
            | NonPortableIncrementDecrement
            | InvalidVariableValue(_)
            | Overflow
            | DivisionByZero
            | LeftShiftingNegative
            | ReverseShifting
            | AssignmentToValue
            | Unrecognized(_) => None,
            UnclosedParenthesis { opening_location } => {
                Some((opening_location, "the opening parenthesis was here"))
            }
            QuestionWithoutColon { question_location } => Some((question_location, "`?` was here")),
        }
    }
}

/// Error expanding an unset variable
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("unset variable `{param}`")]
struct UnsetVariable {
    param: Param,
}

/// Converts `yash_arith::ErrorCause` into `initial::ErrorCause`.
///
/// The `source` argument must be the arithmetic expression being expanded.
/// It is used to reproduce a location contained in the error cause.
#[must_use]
fn convert_error_cause(
    cause: yash_arith::ErrorCause<UnsetVariable, AssignReadOnlyError>,
    source: &Rc<Code>,
) -> ErrorCause {
    use ArithError::*;
    use yash_arith::{
        ErrorCause as EC, EvalError as EE, PortabilityError as PE, SyntaxError as SE,
        TokenError as TE,
    };
    match cause {
        EC::SyntaxError(SE::TokenError(TE::InvalidNumericConstant)) => {
            ErrorCause::ArithError(InvalidNumericConstant)
        }
        EC::SyntaxError(SE::TokenError(TE::InvalidCharacter)) => {
            ErrorCause::ArithError(InvalidCharacter)
        }
        EC::SyntaxError(SE::IncompleteExpression) => ErrorCause::ArithError(IncompleteExpression),
        EC::SyntaxError(SE::MissingOperator) => ErrorCause::ArithError(MissingOperator),
        EC::SyntaxError(SE::UnclosedParenthesis { opening_location }) => {
            let opening_location = Location {
                code: Rc::clone(source),
                range: opening_location,
            };
            ErrorCause::ArithError(UnclosedParenthesis { opening_location })
        }
        EC::SyntaxError(SE::QuestionWithoutColon { question_location }) => {
            let question_location = Location {
                code: Rc::clone(source),
                range: question_location,
            };
            ErrorCause::ArithError(QuestionWithoutColon { question_location })
        }
        EC::SyntaxError(SE::ColonWithoutQuestion) => ErrorCause::ArithError(ColonWithoutQuestion),
        EC::SyntaxError(SE::InvalidOperator) => ErrorCause::ArithError(InvalidOperator),
        EC::PortabilityError(PE::IncrementDecrement) => {
            ErrorCause::ArithError(NonPortableIncrementDecrement)
        }
        EC::EvalError(EE::InvalidVariableValue(value)) => {
            ErrorCause::ArithError(InvalidVariableValue(value))
        }
        EC::EvalError(EE::Overflow) => ErrorCause::ArithError(Overflow),
        EC::EvalError(EE::DivisionByZero) => ErrorCause::ArithError(DivisionByZero),
        EC::EvalError(EE::LeftShiftingNegative) => ErrorCause::ArithError(LeftShiftingNegative),
        EC::EvalError(EE::ReverseShifting) => ErrorCause::ArithError(ReverseShifting),
        EC::EvalError(EE::AssignmentToValue) => ErrorCause::ArithError(AssignmentToValue),
        EC::EvalError(EE::GetVariableError(UnsetVariable { param })) => {
            ErrorCause::UnsetParameter { param }
        }
        EC::EvalError(EE::AssignVariableError(e)) => ErrorCause::AssignReadOnly(e),
        cause => ErrorCause::ArithError(Unrecognized(cause.to_string())),
    }
}

struct VarEnv<'a, S> {
    env: &'a mut yash_env::Env<S>,
    expression: &'a str,
    expansion_location: &'a Location,
}

impl<S> yash_arith::Env for VarEnv<'_, S> {
    type GetVariableError = UnsetVariable;
    type AssignVariableError = AssignReadOnlyError;

    fn get_variable(&self, name: &str) -> Result<Option<&str>, UnsetVariable> {
        match self.env.variables.get_scalar(name) {
            Some(value) => Ok(Some(value)),
            None => match self.env.options.get(Unset) {
                // TODO If the variable exists but is not scalar, UnsetVariable
                // does not seem to be the right error.
                Off => Err(UnsetVariable {
                    param: Param::variable(name),
                }),
                On => Ok(None),
            },
        }
    }

    fn assign_variable(
        &mut self,
        name: &str,
        value: String,
        range: Range<usize>,
    ) -> Result<(), AssignReadOnlyError> {
        let code = Rc::new(Code {
            value: self.expression.to_string().into(),
            start_line_number: 1.try_into().unwrap(),
            source: Source::Arith {
                original: self.expansion_location.clone(),
            }
            .into(),
        });
        self.env
            .get_or_create_variable(name, Global)
            .assign(value, Location { code, range })
            .map(drop)
            .map_err(|e| AssignReadOnlyError {
                name: name.to_owned(),
                new_value: e.new_value,
                read_only_location: e.read_only_location,
                vacancy: None,
            })
    }
}

pub async fn expand<S: Runtime + 'static>(
    text: &Text,
    location: &Location,
    env: &mut Env<'_, S>,
) -> Result<Phrase, Error> {
    let (expression, exit_status) = expand_text(env.inner, text).await?;
    if exit_status.is_some() {
        env.last_command_subst_exit_status = exit_status;
    }

    let mut config = Config::new();
    config.portable = env.inner.options.get(Portable) == On;
    let result = eval_with_config(
        &expression,
        &mut VarEnv {
            env: env.inner,
            expression: &expression,
            expansion_location: location,
        },
        config,
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
                }
                .into(),
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
    use crate::tests::return_builtin;
    use futures_util::FutureExt as _;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::Errno;
    use yash_env::test_helper::in_virtual_system;
    use yash_env::variable::Scope::Global;
    use yash_env::variable::Value::Scalar;

    #[test]
    fn unrecognized_error() {
        let error = ArithError::Unrecognized("new arithmetic error".into());

        assert_eq!(error.to_string(), "new arithmetic error");
        assert_eq!(error.related_location(), None);
    }

    #[test]
    fn var_env_get_variable_success() {
        use yash_arith::Env as _;
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .get_or_new("v", Global)
            .assign("value", None)
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
        use yash_arith::Env as _;
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
        use yash_arith::Env as _;
        let mut env = yash_env::Env::new_virtual();
        env.options.set(Unset, Off);
        let location = Location::dummy("my location");
        let env = VarEnv {
            env: &mut env,
            expression: "0+v",
            expansion_location: &location,
        };

        let result = env.get_variable("v");
        assert_eq!(
            result,
            Err(UnsetVariable {
                param: Param::variable("v")
            })
        );
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
    fn increment_decrement_allowed_without_portable_option() {
        let text = "foo++".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env2 = Env::new(&mut env);

        let result = expand(&text, &location, &mut env2).now_or_never().unwrap();

        assert!(result.is_ok());
        assert_eq!(env.variables.get_scalar("foo"), Some("1"));
    }

    #[test]
    fn increment_decrement_rejected_with_portable_option() {
        let text = "foo++".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        env.options.set(Portable, On);
        let mut env = Env::new(&mut env);

        let result = expand(&text, &location, &mut env).now_or_never().unwrap();

        let error = result.unwrap_err();
        assert_eq!(
            error.cause,
            ErrorCause::ArithError(ArithError::NonPortableIncrementDecrement)
        );
        assert_eq!(*error.location.code.value.borrow(), "foo++");
        assert_eq!(error.location.range, 3..5);
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
        assert_eq!(*location2.code.source, Source::Arith { original: location });
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
        assert_eq!(
            *e.location.code.source,
            Source::Arith { original: location }
        );
        assert_eq!(e.location.range, 0..2);
    }
}
