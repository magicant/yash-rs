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
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::ReadOnlyError;
use yash_env::variable::Scope;
use yash_env::variable::Variable;
use yash_env::Env;

/// Implementation of the readonly built-in.
pub fn builtin_main_sync(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO support options
    // TODO print read-only variables if there are no operands

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let var_value = value[eq_index + 1..].to_owned();
            let var = Variable::new(var_value)
                .set_assigned_location(origin.clone())
                .make_read_only(origin);
            // TODO Apply all-export option

            let mut name = value;
            name.truncate(eq_index);
            // TODO reject invalid name

            match env.variables.assign(Scope::Global, name, var) {
                Ok(_old_value) => (),
                Err(ReadOnlyError {
                    name,
                    read_only_location: _,
                    new_value: _,
                }) => {
                    // TODO Better error message
                    // TODO Use Env rather than printing directly to stderr
                    eprintln!("cannot assign to read-only variable {}", name);
                    return (ExitStatus::FAILURE, Continue(()));
                }
            }
        } else {
            // TODO Make an existing variable read-only or create a new value-less variable
        }
    }

    (ExitStatus::SUCCESS, Continue(()))
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
    use yash_env::variable::Scalar;
    use yash_env::Env;

    #[test]
    fn builtin_defines_read_only_variable() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["foo=bar baz"]);
        let location = args[0].origin.clone();

        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let v = env.variables.get("foo").unwrap();
        assert_eq!(v.value, Scalar("bar baz".to_string()));
        assert_eq!(v.is_exported, false);
        assert_eq!(v.read_only_location.as_ref().unwrap(), &location);
        assert_eq!(v.last_assigned_location.as_ref().unwrap(), &location);
    }
}
