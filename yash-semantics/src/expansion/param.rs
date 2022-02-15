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

//! Parameter expansion semantics.

use super::Env;
use super::Expand;
use super::Expansion;
use super::Origin;
use super::Output;
use super::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use std::num::IntErrorKind::PosOverflow;
use yash_env::variable::Value;
use yash_syntax::source::Span;
use yash_syntax::syntax::Param;

enum ParamValue<'a> {
    Unset,
    Scalar(Cow<'a, str>),
    Array(Cow<'a, [String]>),
}

impl From<String> for ParamValue<'static> {
    fn from(value: String) -> Self {
        ParamValue::Scalar(Cow::Owned(value))
    }
}

impl<'a> From<&'a str> for ParamValue<'a> {
    fn from(value: &'a str) -> Self {
        ParamValue::Scalar(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for ParamValue<'a> {
    fn from(value: &'a String) -> Self {
        ParamValue::Scalar(Cow::Borrowed(value))
    }
}

impl From<Vec<String>> for ParamValue<'static> {
    fn from(values: Vec<String>) -> Self {
        ParamValue::Array(Cow::Owned(values))
    }
}

impl<'a> From<&'a [String]> for ParamValue<'a> {
    fn from(values: &'a [String]) -> Self {
        ParamValue::Array(Cow::Borrowed(values))
    }
}

impl<'a> From<&'a Vec<String>> for ParamValue<'a> {
    fn from(values: &'a Vec<String>) -> Self {
        ParamValue::Array(Cow::Borrowed(values))
    }
}

impl From<Value> for ParamValue<'static> {
    fn from(value: Value) -> Self {
        match value {
            Value::Scalar(value) => ParamValue::from(value),
            Value::Array(values) => ParamValue::from(values),
        }
    }
}

impl<'a> From<&'a Value> for ParamValue<'a> {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::Scalar(value) => ParamValue::from(value),
            Value::Array(values) => ParamValue::from(values),
        }
    }
}

/// Expands a special parameter.
///
/// Returns `None` if the `name` is not a special parameter name.
fn expand_special_parameter<'a, E: Env>(env: &'a mut E, name: &str) -> Option<ParamValue<'a>> {
    let mut chars = name.chars();
    let name_char = chars.next()?;
    if chars.next().is_some() {
        // A special parameter's name is always a single character.
        return None;
    }
    match name_char {
        '@' => todo!(),
        '*' => todo!(),
        '#' => todo!(),
        '?' => Some(env.exit_status().to_string().into()),
        '-' => todo!(),
        '$' => todo!(),
        '!' => Some(env.last_async_pid().to_string().into()),
        '0' => todo!(),
        _ => None,
    }
}

/// Expands a positional parameter.
///
/// Returns `None` if the `name` is not a positive integer.
fn expand_positional_param<'a, E: Env>(env: &'a E, name: &str) -> Option<ParamValue<'a>> {
    let index_0 = match name.parse::<usize>() {
        Ok(index_1) if index_1 > 0 => index_1 - 1,
        Err(error) if error.kind() == &PosOverflow => return Some(ParamValue::Unset),
        _ => return None, // Not a positional parameter
    };
    let params = env.positional_params();
    match &params.value {
        Value::Scalar(value) => match index_0 {
            0 => Some(ParamValue::from(value)),
            _ => Some(ParamValue::Unset),
        },
        Value::Array(values) => match values.get(index_0) {
            Some(value) => Some(ParamValue::from(value)),
            None => Some(ParamValue::Unset),
        },
    }
}

fn expand_variable<'a, E: Env>(env: &'a E, name: &str) -> ParamValue<'a> {
    match env.get_variable(name) {
        Some(v) => ParamValue::from(&v.value),
        None => ParamValue::Unset,
    }
}

/// Reference to a `RawParam` or `BracedParam`.
pub struct ParamRef<'a> {
    name: &'a str,
    #[allow(unused)] // TODO Use this
    span: &'a Span,
}

impl<'a> ParamRef<'a> {
    pub fn from_name_and_location(name: &'a str, span: &'a Span) -> Self {
        ParamRef { name, span }
    }
}

impl<'a> From<&'a Param> for ParamRef<'a> {
    fn from(param: &'a Param) -> Self {
        ParamRef {
            name: &param.name,
            span: &param.span,
        }
    }
}

#[async_trait(?Send)]
impl Expand for ParamRef<'_> {
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        let value = if let Some(value) = expand_special_parameter(env, self.name) {
            value
        } else {
            expand_positional_param(env, self.name)
                .unwrap_or_else(|| expand_variable(env, self.name))
        };
        match value {
            ParamValue::Unset => (),
            ParamValue::Scalar(value) => {
                output.push_str(&value, Origin::SoftExpansion, false, false)
            }
            ParamValue::Array(values) => {
                todo!("expand array values: {:?}", values)
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::AttrChar;
    use super::*;
    use futures_executor::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use yash_env::io::Fd;
    use yash_env::job::Pid;
    use yash_env::job::WaitStatus;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::Errno;
    use yash_env::variable::ReadOnlyError;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env::variable::Variable;
    use yash_env::VirtualSystem;

    #[derive(Debug)]
    struct Singleton {
        name: String,
        value: Variable,
    }

    #[async_trait(?Send)]
    impl Env for Singleton {
        fn get_variable(&self, name: &str) -> Option<&Variable> {
            if name == self.name {
                Some(&self.value)
            } else {
                None
            }
        }
        fn assign_variable(
            &mut self,
            _scope: Scope,
            name: String,
            value: Variable,
        ) -> std::result::Result<Option<Variable>, ReadOnlyError> {
            if self.name == name {
                if let Some(location) = &self.value.read_only_location {
                    return Err(ReadOnlyError {
                        name,
                        read_only_location: location.clone(),
                        new_value: value,
                    });
                }
                Ok(Some(std::mem::replace(&mut self.value, value)))
            } else {
                self.name = name;
                Ok(None)
            }
        }
        fn positional_params(&self) -> &Variable {
            unimplemented!("not available for Singleton");
        }
        fn positional_params_mut(&mut self) -> &mut Variable {
            unimplemented!("not available for Singleton");
        }
        fn exit_status(&self) -> yash_env::semantics::ExitStatus {
            unimplemented!("not available for Singleton");
        }
        fn save_command_subst_exit_status(&mut self, _: ExitStatus) {
            unimplemented!("not available for Singleton");
        }
        fn last_async_pid(&mut self) -> yash_env::job::Pid {
            unimplemented!("not available for Singleton");
        }
        fn pipe(&mut self) -> std::result::Result<(Fd, Fd), Errno> {
            unimplemented!("not available for Singleton");
        }
        fn dup2(&mut self, _from: Fd, _to: Fd) -> std::result::Result<Fd, Errno> {
            unimplemented!("not available for Singleton");
        }
        fn close(&mut self, _fd: Fd) -> std::result::Result<(), Errno> {
            unimplemented!("not available for Singleton");
        }
        async fn read_async(
            &mut self,
            _fd: Fd,
            _buffer: &mut [u8],
        ) -> std::result::Result<usize, Errno> {
            unimplemented!("not available for Singleton");
        }
        async fn start_subshell<F>(&mut self, _f: F) -> std::result::Result<Pid, Errno>
        where
            F: for<'a> FnOnce(
                    &'a mut yash_env::Env,
                )
                    -> Pin<Box<dyn Future<Output = yash_env::semantics::Result> + 'a>>
                + 'static,
        {
            unimplemented!("not available for Singleton");
        }
        async fn wait_for_subshell(
            &mut self,
            _target: Pid,
        ) -> std::result::Result<WaitStatus, Errno> {
            unimplemented!("not available for Singleton");
        }
    }

    #[derive(Debug)]
    struct PositionalParams(Variable);

    #[async_trait(?Send)]
    impl Env for PositionalParams {
        fn get_variable(&self, _name: &str) -> Option<&Variable> {
            unimplemented!("not available for PositionalParams");
        }
        fn assign_variable(
            &mut self,
            _scope: Scope,
            _name: String,
            _value: Variable,
        ) -> std::result::Result<Option<Variable>, ReadOnlyError> {
            unimplemented!("not available for PositionalParams");
        }
        fn positional_params(&self) -> &Variable {
            &self.0
        }
        fn positional_params_mut(&mut self) -> &mut Variable {
            &mut self.0
        }
        fn exit_status(&self) -> yash_env::semantics::ExitStatus {
            unimplemented!("not available for PositionalParams");
        }
        fn save_command_subst_exit_status(&mut self, _: ExitStatus) {
            unimplemented!("not available for PositionalParams");
        }
        fn last_async_pid(&mut self) -> yash_env::job::Pid {
            unimplemented!("not available for PositionalParams");
        }
        fn pipe(&mut self) -> std::result::Result<(Fd, Fd), Errno> {
            unimplemented!("not available for PositionalParams");
        }
        fn dup2(&mut self, _from: Fd, _to: Fd) -> std::result::Result<Fd, Errno> {
            unimplemented!("not available for PositionalParams");
        }
        fn close(&mut self, _fd: Fd) -> std::result::Result<(), Errno> {
            unimplemented!("not available for PositionalParams");
        }
        async fn read_async(
            &mut self,
            _fd: Fd,
            _buffer: &mut [u8],
        ) -> std::result::Result<usize, Errno> {
            unimplemented!("not available for PositionalParams");
        }
        async fn start_subshell<F>(&mut self, _f: F) -> std::result::Result<Pid, Errno>
        where
            F: for<'a> FnOnce(
                    &'a mut yash_env::Env,
                )
                    -> Pin<Box<dyn Future<Output = yash_env::semantics::Result> + 'a>>
                + 'static,
        {
            unimplemented!("not available for PositionalParams");
        }
        async fn wait_for_subshell(
            &mut self,
            _target: Pid,
        ) -> std::result::Result<WaitStatus, Errno> {
            unimplemented!("not available for PositionalParams");
        }
    }

    #[test]
    fn name_only_param_unset() {
        let name = "foo".to_string();
        let value = Variable {
            value: Value::Scalar("!".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        };
        let mut env = Singleton { name, value };
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let span = Span::dummy("");
        let p = ParamRef::from_name_and_location("!bar", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(field, []);
    }

    #[test]
    fn name_only_param_existing_variable() {
        let name = "foo".to_string();
        let value = Variable {
            value: Value::Scalar("ok".to_string()),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        };
        let mut env = Singleton { name, value };
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let span = Span::dummy("");
        let p = ParamRef::from_name_and_location("foo", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [
                AttrChar {
                    value: 'o',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                },
                AttrChar {
                    value: 'k',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                }
            ]
        );
    }

    #[test]
    fn name_only_param_existing_positional() {
        let mut env = PositionalParams(Variable {
            value: Value::Array(vec!["a".to_string(), "b".to_string()]),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        });
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let span = Span::dummy("");
        let p = ParamRef::from_name_and_location("1", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [AttrChar {
                value: 'a',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            }]
        );

        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let p = ParamRef::from_name_and_location("2", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [AttrChar {
                value: 'b',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            }]
        );
    }

    #[test]
    fn name_only_param_unset_positional() {
        let mut env = PositionalParams(Variable {
            value: Value::Array(vec!["1".to_string()]),
            last_assigned_location: None,
            is_exported: false,
            read_only_location: None,
        });
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let span = Span::dummy("");
        let p = ParamRef::from_name_and_location("2", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(field, []);

        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let p = ParamRef::from_name_and_location("3", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(field, []);

        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let p = ParamRef::from_name_and_location("9999999999999999999999999999999999999999", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(field, []);
    }

    #[test]
    fn name_only_param_special_parameter_exit_status() {
        let system = VirtualSystem::new();
        let mut env = yash_env::Env::with_system(Box::new(system));
        env.exit_status = ExitStatus(56);
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let span = Span::dummy("");
        let p = ParamRef::from_name_and_location("?", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [
                AttrChar {
                    value: '5',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                },
                AttrChar {
                    value: '6',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                }
            ]
        );
    }

    #[test]
    fn name_only_param_special_parameter_last_async_pid() {
        let system = VirtualSystem::new();
        let mut env = yash_env::Env::with_system(Box::new(system));
        env.jobs.set_last_async_pid(Pid::from_raw(72));
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let span = Span::dummy("");
        let p = ParamRef::from_name_and_location("!", &span);
        block_on(p.expand(&mut env, &mut output)).unwrap();
        assert_eq!(
            field,
            [
                AttrChar {
                    value: '7',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                },
                AttrChar {
                    value: '2',
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                }
            ]
        );
    }
}
