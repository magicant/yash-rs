// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Help built-in
//!
//! This module implements the `help` built-in, which prints the help message
//! of built-ins.
//!
//! The help message of a built-in is obtained from [`Builtin::help`] of the
//! built-in registered in [`Env::builtins`], so the built-in can describe
//! built-ins defined outside this crate as well.
//!
//! [`Builtin::help`]: yash_env::builtin::Builtin::help

use crate::common::help::{HELP_OPTION, print};
use crate::common::report::report_error;
use crate::common::syntax::{Mode, OptionSpec, parse_arguments};
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

const OPTION_SPECS: &[OptionSpec] = &[HELP_OPTION];

/// Entry point of the `help` built-in
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: Isatty + WriteAll,
{
    let (options, operands) = match parse_arguments(OPTION_SPECS, Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };

    if !options.is_empty() {
        todo!("help --help");
    }
    let [operand] = operands.as_slice() else {
        todo!("help with no or multiple operands");
    };
    let Some(help) = env
        .builtins
        .get(operand.value.as_str())
        .and_then(|builtin| builtin.help)
    else {
        todo!("help for a name without help");
    };
    print(env, help).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::builtin::{Builtin, Help as _, Type};
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stdout;

    fn env_with_pwd() -> (Env<Rc<Concurrent<VirtualSystem>>>, VirtualSystem) {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        let mut builtin = Builtin::new(Type::Substitutive, |_, _| unreachable!());
        builtin.help = Some(&crate::pwd::HELP);
        env.builtins.insert("pwd", builtin);
        (env, system)
    }

    #[test]
    fn prints_help_of_operand() {
        let (mut env, system) = env_with_pwd();

        let result = main(&mut env, Field::dummies(["pwd"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, crate::pwd::HELP.message())
        });
    }
}
