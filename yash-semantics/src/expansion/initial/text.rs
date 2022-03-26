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
use super::QuickExpand::{self, Interim, Ready};
use async_trait::async_trait;
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
///   string is [parsed and executed](crate::read_eval_loop) in a subshell where
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
///   command](yash_env::job::JobSet::expand_last_async_pid).
/// - `@` expands to all positional parameters. When expanded in double-quotes
///   as in `"${@}"`, it produces the correct number of fields exactly matching
///   the current positional parameters. Especially if there are zero positional
///   parameters, it expands to zero fields.
///   (TODO: Elaborate on how that works in this function)
/// - `*` expands to all positional parameters. When expanded in double-quotes
///   as in `"${*}"`, the result is a concatenation of all the positional
///   parameters, each separated by the first character of the `IFS` variable
///   (or by a space if the variable is unset, or by nothing if it is an empty
///   string). When expanded outside double-quotes, `*` expands the same as `@`.
///   (TODO: Elaborate on how that works in this function)
/// - `#` expands to the number of positional parameters.
/// - `-` TODO Elaborate
/// - `$` expands to the process ID of the main shell process
///   ([`Env::main_pid`](yash_env::Env::main_pid)). Note that this value does
///   _not_ change in subshells.
/// - `0` TODO Elaborate
///
/// TODO Elaborate on index and modifiers
#[async_trait(?Send)]
impl Expand for TextUnit {
    type Interim = ();

    fn quick_expand(&self, _env: &mut Env<'_>) -> QuickExpand<Self> {
        match self {
            &Literal(value) => Ready(Ok(Phrase::Char(AttrChar {
                value,
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false,
            }))),
            &Backslashed(value) => Ready(Ok(Phrase::Field(vec![
                AttrChar {
                    value: '\\',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                },
                AttrChar {
                    value,
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                },
            ]))),
            RawParam { .. } | BracedParam(_) | CommandSubst { .. } | Backquote { .. } => {
                Interim(())
            }
            Arith { .. } => todo!(),
        }
    }

    async fn async_expand(&self, env: &mut Env<'_>, (): ()) -> Result<Phrase, Error> {
        match self {
            Literal(_) => unimplemented!("async_expand not expecting Literal"),
            Backslashed(_) => unimplemented!("async_expand not expecting Backslashed"),
            RawParam { name, location } => {
                let modifier = &yash_syntax::syntax::Modifier::None;
                let param = ParamRef {
                    name,
                    modifier,
                    location,
                };
                param.expand(env).await
            }
            BracedParam(param) => ParamRef::from(param).expand(env).await,
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
            Arith { .. } => todo!(),
        }
    }
}

/// Expands a text.
///
/// This implementation delegates to `[TextUnit] as Expand`.
#[async_trait(?Send)]
impl Expand for Text {
    type Interim = <[TextUnit] as Expand>::Interim;

    #[inline]
    fn quick_expand(&self, env: &mut Env<'_>) -> QuickExpand<Self> {
        match self.0.quick_expand(env) {
            Ready(result) => Ready(result),
            Interim(interim) => Interim(interim),
        }
    }

    #[inline]
    async fn async_expand(
        &self,
        env: &mut Env<'_>,
        interim: Self::Interim,
    ) -> Result<Phrase, Error> {
        self.0.async_expand(env, interim).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env::variable::Variable;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::Modifier;
    use yash_syntax::syntax::Param;

    #[test]
    fn literal() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        assert_matches!(Literal('L').quick_expand(&mut env), Ready(result) => {
            let c = AttrChar {
                value: 'L',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false,
            };
            assert_eq!(result, Ok(Phrase::Char(c)));
        });
    }

    #[test]
    fn backslashed() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        assert_matches!(Backslashed('L').quick_expand(&mut env), Ready(result) => {
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
        });
    }

    #[test]
    fn raw_param() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable {
                    value: Value::Scalar("x".to_string()),
                    last_assigned_location: None,
                    is_exported: false,
                    read_only_location: None,
                },
            )
            .unwrap();
        let mut env = Env::new(&mut env);
        let name = "foo".to_string();
        let location = Location::dummy("");
        let param = RawParam { name, location };
        assert_matches!(param.quick_expand(&mut env), Interim(()));
        let result = param.async_expand(&mut env, ()).now_or_never().unwrap();
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
            .assign(
                Scope::Global,
                "foo".to_string(),
                Variable {
                    value: Value::Scalar("x".to_string()),
                    last_assigned_location: None,
                    is_exported: false,
                    read_only_location: None,
                },
            )
            .unwrap();
        let mut env = Env::new(&mut env);
        let param = BracedParam(Param {
            name: "foo".to_string(),
            modifier: Modifier::None,
            location: Location::dummy(""),
        });
        assert_matches!(param.quick_expand(&mut env), Interim(()));
        let result = param.async_expand(&mut env, ()).now_or_never().unwrap();
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
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let mut env = Env::new(&mut env);
            let subst = TextUnit::CommandSubst {
                content: "echo .".to_string(),
                location: Location::dummy(""),
            };
            assert_matches!(subst.quick_expand(&mut env), Interim(()));
            let result = subst.async_expand(&mut env, ()).await;
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
        in_virtual_system(|mut env, _pid, _state| async move {
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
            assert_matches!(subst.quick_expand(&mut env), Interim(()));
            let result = subst.async_expand(&mut env, ()).await;
            let c = AttrChar {
                value: '\\',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            };
            assert_eq!(result, Ok(Phrase::Char(c)));
        })
    }
}
