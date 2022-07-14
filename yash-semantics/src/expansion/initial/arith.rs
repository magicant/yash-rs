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

use std::rc::Rc;

use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::super::phrase::Phrase;
use super::super::ErrorCause;
use super::Env;
use super::Error;
use crate::expansion::expand_text;
use yash_arith::eval;
use yash_syntax::source::Code;
use yash_syntax::source::Location;
use yash_syntax::source::Source;
use yash_syntax::syntax::Text;

pub async fn expand(text: &Text, location: &Location, env: &mut Env<'_>) -> Result<Phrase, Error> {
    let (expression, exit_status) = expand_text(env.inner, text).await?;
    if exit_status.is_some() {
        env.last_command_subst_exit_status = exit_status;
    }

    let result = eval(&expression);

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
        Err(error) => Err(Error {
            cause: ErrorCause::ArithError(error.cause),
            location: Location {
                code: Rc::new(Code {
                    value: expression.into(),
                    start_line_number: 1.try_into().unwrap(),
                    source: Source::Arith {
                        original: location.clone(),
                    },
                }),
                range: error.location,
            },
        }),
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

    #[test]
    fn successful_inner_text_expansion() {
        let text = "0".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(&text, &location, &mut env).now_or_never().unwrap();
        let c = AttrChar {
            value: '0',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
        assert_eq!(env.last_command_subst_exit_status, None);
    }

    #[test]
    fn non_zero_exit_status_from_inner_text_expansion() {
        in_virtual_system(|mut env, _, _| async move {
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
    fn error_in_arithmetic_evaluation() {
        let text = "09".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(&text, &location, &mut env).now_or_never().unwrap();
        let e = result.unwrap_err();
        assert_eq!(
            e.cause,
            ErrorCause::ArithError(yash_arith::ErrorCause::InvalidNumericConstant)
        );
        assert_eq!(*e.location.code.value.borrow(), "09");
        assert_eq!(e.location.code.source, Source::Arith { original: location });
        assert_eq!(e.location.range, 0..2);
    }
}
