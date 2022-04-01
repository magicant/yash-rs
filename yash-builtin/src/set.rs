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

//! Set built-in
//!
//! TODO Elaborate

use crate::common::BuiltinName;
use std::future::ready;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::Array;
use yash_env::Env;

/// Implementation of the set built-in.
pub fn builtin_main_sync(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO Parse options
    // TODO Print existing variables when no arguments are given

    let location = env.builtin_name().origin.clone();
    let params = env.variables.positional_params_mut();
    params.value = Array(args.into_iter().map(|f| f.value).collect());
    params.last_assigned_location = Some(location);

    (ExitStatus::SUCCESS, Continue(()))
}

/// Implementation of the set built-in.
///
/// This function calls [`builtin_main_sync`] and wraps the result in a `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::stack::Frame;

    #[test]
    fn setting_some_positional_parameters() {
        let name = Field::dummy("set");
        let location = name.origin.clone();
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(Frame::Builtin { name });
        let args = Field::dummies(["a", "b", "z"]);

        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let v = env.variables.positional_params();
        assert_eq!(
            v.value,
            Array(vec!["a".to_string(), "b".to_string(), "z".to_string()])
        );
        assert_eq!(v.read_only_location, None);
        assert_eq!(v.last_assigned_location.as_ref().unwrap(), &location);
    }
}
