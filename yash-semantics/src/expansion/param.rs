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
use yash_env::variable::Value;
use yash_syntax::source::Location;
use yash_syntax::syntax::Param;

/// Reference to a `RawParam` or `BracedParam`.
pub struct ParamRef<'a> {
    name: &'a str,
    #[allow(unused)] // TODO Use this
    location: &'a Location,
}

impl<'a> ParamRef<'a> {
    pub fn from_name_and_location(name: &'a str, location: &'a Location) -> Self {
        ParamRef { name, location }
    }
}

impl<'a> From<&'a Param> for ParamRef<'a> {
    fn from(param: &'a Param) -> Self {
        ParamRef {
            name: &param.name,
            location: &param.location,
        }
    }
}

#[async_trait(?Send)]
impl Expand for ParamRef<'_> {
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        if let Some(v) = expand_special_parameter(env, self.name) {
            output.push_str(&v, Origin::SoftExpansion, false, false);
        } else if let Some(v) = env.get_variable(self.name) {
            match &v.value {
                Value::Scalar(value) => output.push_str(value, Origin::SoftExpansion, false, false),
                Value::Array(values) => todo!("expand array values: {:?}", values),
            }
        }
        Ok(())
    }
}

fn expand_special_parameter<E: Env>(env: &mut E, name: &str) -> Option<String> {
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
        '?' => Some(env.exit_status().to_string()),
        '-' => todo!(),
        '$' => todo!(),
        '!' => Some(env.last_async_pid().to_string()),
        '0' => todo!(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::AttrChar;
    use super::*;
    use futures_executor::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use yash_env::exec::ExitStatus;
    use yash_env::io::Fd;
    use yash_env::job::Pid;
    use yash_env::job::WaitStatus;
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
        fn exit_status(&self) -> yash_env::exec::ExitStatus {
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
                    -> Pin<Box<dyn Future<Output = yash_env::exec::Result> + 'a>>
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
        let location = Location::dummy("");
        let p = ParamRef::from_name_and_location("!bar", &location);
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
        let location = Location::dummy("");
        let p = ParamRef::from_name_and_location("foo", &location);
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
    fn name_only_param_special_parameter_exit_status() {
        let system = VirtualSystem::new();
        let mut env = yash_env::Env::with_system(Box::new(system));
        env.exit_status = ExitStatus(56);
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let location = Location::dummy("");
        let p = ParamRef::from_name_and_location("?", &location);
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
        let location = Location::dummy("");
        let p = ParamRef::from_name_and_location("!", &location);
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
