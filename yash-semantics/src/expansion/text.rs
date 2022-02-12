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

//! Initial expansion of text.

use super::command_subst::CommandSubstRef;
use super::param::ParamRef;
use super::AttrChar;
use super::Env;
use super::Expand;
use super::Expansion;
use super::Origin;
use super::Output;
use super::Result;
use async_trait::async_trait;
use yash_syntax::source::Location;
use yash_syntax::syntax::Text;
use yash_syntax::syntax::TextUnit;
use yash_syntax::syntax::Unquote;

#[async_trait(?Send)]
impl Expand for TextUnit {
    /// Expands the text unit.
    ///
    /// - `Literal` expands to its character value.
    /// - `Backslashed` expands to two characters: a quoting backslash (`\`)
    ///   followed by its quoted character value.
    /// - `RawParam` and `BracedParam` perform parameter expansion, detailed
    ///   below.
    /// - `CommandSubst` and `Backquote` perform command substitution: The
    ///   `content` string is [parsed and executed](crate::read_eval_loop) in a
    ///   subshell where its standard output is redirected to a pipe read by the
    ///   shell. The substitution expands to the output with trailing newlines
    ///   removed.
    /// - `Arith` performs arithmetic expansion: The content text is expanded,
    ///   parsing the result as an arithmetic expression. The evaluated value of
    ///   the expression will be the final result of the expansion.
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
    /// - `@` expands to all positional parameters. When expanded in
    ///   double-quotes as in `"${@}"`, it produces the correct number of fields
    ///   exactly matching the current positional parameters. Especially if
    ///   there are zero positional parameters, it expands to zero fields.
    ///   (TODO: Elaborate on how that works in this function)
    /// - `*` expands to all positional parameters. When expanded in
    ///   double-quotes as in `"${*}"`, the result is a concatenation of all the
    ///   positional parameters, each separated by the first character of the
    ///   `IFS` variable (or by a space if the variable is unset, or by nothing
    ///   if it is an empty string). When expanded outside double-quotes, `*`
    ///   expands the same as `@`.
    ///   (TODO: Elaborate on how that works in this function)
    /// - `#` expands to the number of positional parameters.
    /// - `-` TODO Elaborate
    /// - `$` expands to the process ID of the main shell process. Note that
    ///   this value does _not_ change in subshells.
    /// - `0` TODO Elaborate
    ///
    /// TODO Elaborate on index and modifiers
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        use TextUnit::*;
        match self {
            Literal(c) => {
                output.push_char(AttrChar {
                    value: *c,
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false,
                });
                Ok(())
            }
            Backslashed(c) => {
                output.push_char(AttrChar {
                    value: '\\',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                });
                output.push_char(AttrChar {
                    value: *c,
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                });
                Ok(())
            }
            RawParam { name, span } => {
                let location = Location {
                    code: span.code.clone(),
                    index: span.range.start,
                };
                let param = ParamRef::from_name_and_location(name, &location);
                param.expand(env, output).await
            }
            BracedParam(param) => ParamRef::from(param).expand(env, output).await,
            CommandSubst { content, location } => {
                CommandSubstRef::new(content, location)
                    .expand(env, output)
                    .await
            }
            Backquote { content, location } => {
                let content = content.unquote().0;
                CommandSubstRef::new(&content, location)
                    .expand(env, output)
                    .await
            }
            // TODO Expand Arith correctly
            _ => {
                output.push_str(&self.to_string(), Origin::Literal, false, false);
                Ok(())
            }
        }
    }
}

#[async_trait(?Send)]
impl Expand for Text {
    /// Expands the text.
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        self.0.expand(env, output).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::AttrChar;
    use super::*;
    use crate::expansion::tests::NullEnv;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use futures_executor::block_on;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::TextUnit;

    #[test]
    fn literal_expand_unquoted() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let l = TextUnit::Literal('&');
        block_on(l.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [AttrChar {
                value: '&',
                origin: Origin::Literal,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn backslashed_expand_unquoted() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let b = TextUnit::Backslashed('$');
        block_on(b.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [
                AttrChar {
                    value: '\\',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: true,
                },
                AttrChar {
                    value: '$',
                    origin: Origin::Literal,
                    is_quoted: true,
                    is_quoting: false,
                }
            ]
        );
    }

    #[test]
    fn command_subst_expand_unquoted() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let mut field = Vec::<AttrChar>::default();
            let mut output = Output::new(&mut field);
            let subst = TextUnit::CommandSubst {
                content: "echo .".to_string(),
                location: Location::dummy(""),
            };
            env.builtins.insert("echo", echo_builtin());
            subst.expand(&mut env, &mut output).await.unwrap();
            assert_eq!(
                field,
                [AttrChar {
                    value: '.',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false
                }]
            );
        })
    }

    #[test]
    fn backquote_expand_unquoted() {
        in_virtual_system(|mut env, _pid, _state| async move {
            use yash_syntax::syntax::BackquoteUnit::*;
            let mut field = Vec::<AttrChar>::default();
            let mut output = Output::new(&mut field);
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
            env.builtins.insert("echo", echo_builtin());
            subst.expand(&mut env, &mut output).await.unwrap();
            assert_eq!(
                field,
                [AttrChar {
                    value: '\\',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false
                }]
            );
        })
    }

    #[test]
    fn text_expand() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut output = Output::new(&mut field);
        let text: Text = "<->".parse().unwrap();
        block_on(text.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [
                AttrChar {
                    value: '<',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: '-',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                },
                AttrChar {
                    value: '>',
                    origin: Origin::Literal,
                    is_quoted: false,
                    is_quoting: false
                }
            ]
        );
    }
}
