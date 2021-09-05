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

//! Readonly built-in.

use std::future::ready;
use std::future::Future;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::exec::ExitStatus;
use yash_env::expansion::Field;
use yash_env::variable::ReadOnlyError;
use yash_env::variable::Scalar;
use yash_env::variable::Variable;

/// Part of the shell execution environment the readonly built-in depends on.
pub trait Env {
    /// Gets a reference to the variable with the specified name.
    #[must_use]
    fn get_variable(&self, name: &str) -> Option<&Variable>;

    /// Assigns a variable.
    fn assign_variable(
        &mut self,
        name: String,
        value: Variable,
    ) -> std::result::Result<Option<Variable>, ReadOnlyError>;

    // TODO stdout, stderr
}

impl Env for yash_env::Env {
    fn get_variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }
    fn assign_variable(
        &mut self,
        name: String,
        value: Variable,
    ) -> std::result::Result<Option<Variable>, ReadOnlyError> {
        self.variables.assign(name, value)
    }
}

/// Implementation of the readonly built-in.
pub fn builtin_main_sync<E: Env>(env: &mut E, args: Vec<Field>) -> Result {
    // TODO support options

    let mut args = args.into_iter();
    args.next(); // ignore the first argument, which is the command name

    // TODO print read-only variables if there are no operands

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let name = value[..eq_index].to_owned();
            // TODO reject invalid name
            let value = value[eq_index + 1..].to_owned();
            let location = origin.clone();
            // TODO Keep the variable exported if already exported
            // TODO Apply all-export option
            let value = Variable {
                value: Scalar(value),
                last_assigned_location: Some(origin),
                is_exported: false,
                read_only_location: Some(location),
            };
            match env.assign_variable(name, value) {
                Ok(_old_value) => (),
                Err(ReadOnlyError {
                    name,
                    read_only_location: _,
                    new_value: _,
                }) => {
                    // TODO Better error message
                    // TODO Use Env rather than printing directly to stderr
                    eprintln!("cannot assign to read-only variable {}", name);
                    return (ExitStatus::FAILURE, None);
                }
            }
        } else {
            // TODO Make an existing variable read-only or create a new value-less variable
        }
    }

    (ExitStatus::SUCCESS, None)
}

/// Implementation of the readonly built-in.
///
/// This function calls [`builtin_main_sync`] and wraps the result in a `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::Env;

    #[test]
    fn builtin_defines_read_only_variable() {
        let mut env = Env::new_virtual();
        let arg0 = Field::dummy("");
        let arg1 = Field::dummy("foo=bar baz");
        let location = arg1.origin.clone();
        let args = vec![arg0, arg1];

        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus::SUCCESS, None));

        let v = env.variables.get("foo").unwrap();
        assert_eq!(v.value, Scalar("bar baz".to_string()));
        assert_eq!(v.is_exported, false);
        assert_eq!(v.read_only_location.as_ref().unwrap(), &location);
        assert_eq!(v.last_assigned_location.as_ref().unwrap(), &location);
    }
}
