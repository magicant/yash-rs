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

//! Initial expansion of texts and text units.

use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::super::Error;
use super::param::ParamRef;
use super::Env;
use super::Expand;
use super::Phrase;
use yash_syntax::syntax::Text;
use yash_syntax::syntax::TextUnit::{self, *};
use yash_syntax::syntax::Unquote;

/// Expands the text unit.
///
/// - `Literal` expands to its character value.
/// - `Backslashed` expands to two characters: a quoting backslash (`\`)
///   followed by its quoted character value.
/// - `RawParam` and `BracedParam` perform parameter expansion, detailed below.
/// - `CommandSubst` and `Backquote` perform command substitution: The `content`
///   string is [parsed and executed](crate::ReadEvalLoop) in a subshell where
///   its standard output is redirected to a pipe read by the shell. The
///   substitution expands to the output with trailing newlines removed.
/// - `Arith` performs arithmetic expansion: The content text is expanded,
///   parsing the result as an arithmetic expression. The evaluated value of the
///   expression will be the final result of the expansion.
///
/// # Parameter expansion
///
/// A parameter expansion expands to the value of a parameter, optionally
/// modified by a modifier.
///
/// The parameter name selects a parameter to expand. If the name is a
/// positive decimal integer, it is regarded as a positional parameter. If
/// the name matches one of the following special parameter symbols, that
/// special parameter is expanded. Otherwise, the name [selects a
/// variable](yash_env::variable::VariableSet::get) from the environment. A
/// non-existent variable expands to an empty string by default.
///
/// - `?` expands to the [last exit status](yash_env::Env::exit_status).
/// - `!` expands to the [process ID of the last asynchronous
///   command](yash_env::job::JobList::last_async_pid).
/// - `@` expands to all positional parameters. When expanded in double-quotes
///   as in `"${@}"`, it produces the correct number of fields exactly matching
///   the current positional parameters. Especially if there are zero positional
///   parameters, it expands to zero fields.
/// - `*` expands to all positional parameters. When expanded in double-quotes
///   as in `"${*}"`, the result is a concatenation of all the positional
///   parameters, each separated by the first character of the `IFS` variable
///   (or by a space if the variable is unset, or by nothing if it is an empty
///   string). When expanded outside double-quotes, `*` expands the same as `@`.
/// - `#` expands to the number of positional parameters.
/// - `-` expands to a string that is a concatenation of the short names of
///   options matching the current option states in the environment.
/// - `$` expands to the process ID of the main shell process
///   ([`Env::main_pid`](yash_env::Env::main_pid)). Note that this value does
///   _not_ change in subshells.
/// - `0` expands to the name of the current shell executable or shell script
///   ([`Env::arg0`](yash_env::Env::arg0)).
///
/// TODO Elaborate on index and modifiers
impl Expand for TextUnit {
    async fn expand(&self, env: &mut Env<'_>) -> Result<Phrase, Error> {
        match self {
            &Literal(value) => Ok(Phrase::Char(AttrChar {
                value,
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false,
            })),

            &Backslashed(value) => {
                let bs = AttrChar {
                    value: '\\',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                };
                let c = AttrChar {
                    value,
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                };
                Ok(Phrase::Field(vec![bs, c]))
            }

            RawParam { param, location } => {
                let param = ParamRef {
                    name: &param.id,
                    modifier: &yash_syntax::syntax::Modifier::None,
                    location,
                };
                Box::pin(param.expand(env)).await // Boxing needed for recursion
            }

            // Boxing needed for recursion
            BracedParam(param) => Box::pin(ParamRef::from(param).expand(env)).await,

            CommandSubst { content, location } => {
                let command = content.clone();
                let location = location.clone();
                super::command_subst::expand(command, location, env).await
            }

            Backquote { content, location } => {
                let command = content.unquote().0;
                let location = location.clone();
                super::command_subst::expand(command, location, env).await
            }

            Arith { content, location } => {
                // Boxing needed for recursion
                Box::pin(super::arith::expand(content, location, env)).await
            }
        }
    }
}

/// Expands a text.
///
/// This implementation delegates to `[TextUnit] as Expand`.
impl Expand for Text {
    async fn expand(&self, env: &mut Env<'_>) -> Result<Phrase, Error> {
        self.0.expand(env).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::param::tests::braced_variable;
    use super::*;
    use crate::tests::echo_builtin;
    use futures_util::FutureExt;
    use yash_env::variable::Scope;
    use yash_env_test_helper::in_virtual_system;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::BracedParam;
    use yash_syntax::syntax::Param;

    #[test]
    fn literal() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = Literal('L').expand(&mut env).now_or_never().unwrap();

        let c = AttrChar {
            value: 'L',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
    }

    #[test]
    fn backslashed() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = Backslashed('L').expand(&mut env).now_or_never().unwrap();

        let bs = AttrChar {
            value: '\\',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let c = AttrChar {
            value: 'L',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Field(vec![bs, c])));
    }

    #[test]
    fn raw_param() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .get_or_new("foo", Scope::Global)
            .assign("x", None)
            .unwrap();
        let mut env = Env::new(&mut env);
        let raw_param = RawParam {
            param: Param::variable("foo"),
            location: Location::dummy(""),
        };
        let result = raw_param.expand(&mut env).now_or_never().unwrap();

        let c = AttrChar {
            value: 'x',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
    }

    #[test]
    fn braced_param() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .get_or_new("foo", Scope::Global)
            .assign("x", None)
            .unwrap();
        let mut env = Env::new(&mut env);
        let param = BracedParam(braced_variable("foo"));
        let result = param.expand(&mut env).now_or_never().unwrap();

        let c = AttrChar {
            value: 'x',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
    }

    #[test]
    fn command_subst() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let mut env = Env::new(&mut env);
            let subst = TextUnit::CommandSubst {
                content: "echo .".into(),
                location: Location::dummy(""),
            };
            let result = subst.expand(&mut env).await;

            let c = AttrChar {
                value: '.',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            };
            assert_eq!(result, Ok(Phrase::Char(c)));
        })
    }

    #[test]
    fn backquote() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let mut env = Env::new(&mut env);
            use yash_syntax::syntax::BackquoteUnit::*;
            let subst = TextUnit::Backquote {
                content: vec![
                    Literal('e'),
                    Literal('c'),
                    Literal('h'),
                    Literal('o'),
                    Literal(' '),
                    Backslashed('\\'),
                    Backslashed('\\'),
                ],
                location: Location::dummy(""),
            };
            let result = subst.expand(&mut env).await;

            let c = AttrChar {
                value: '\\',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            };
            assert_eq!(result, Ok(Phrase::Char(c)));
        })
    }

    #[test]
    fn arithmetic() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let arith = TextUnit::Arith {
            content: "1+2*3".parse().unwrap(),
            location: Location::dummy(""),
        };
        let result = arith.expand(&mut env).now_or_never().unwrap();

        let c = AttrChar {
            value: '7',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
    }
}
