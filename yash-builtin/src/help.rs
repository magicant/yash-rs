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
//! The help information of built-ins is kept in a table stored in
//! [`Env::any`]. The table initially contains the help of the built-ins
//! defined in this crate. Built-ins defined outside this crate can be given
//! help with [`register`].

use crate::common::help::{HELP_OPTION, Help, print};
use crate::common::report::report_error;
use crate::common::syntax::{Mode, OptionSpec, parse_arguments};
use std::collections::HashMap;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

const OPTION_SPECS: &[OptionSpec] = &[HELP_OPTION];

/// Table of help information keyed by the name of the built-in
#[derive(Clone)]
struct HelpTable(HashMap<&'static str, &'static dyn Help>);

impl Default for HelpTable {
    fn default() -> Self {
        Self(HashMap::from([("pwd", &crate::pwd::HELP as &dyn Help)]))
    }
}

fn table<S>(env: &mut Env<S>) -> &mut HashMap<&'static str, &'static dyn Help> {
    &mut env.any.get_or_insert_with(Box::<HelpTable>::default).0
}

/// Registers the help information of a built-in.
///
/// The `help` built-in prints the registered help for the built-in named
/// `name`. If help has already been registered for the name, it is replaced.
pub fn register<S>(env: &mut Env<S>, name: &'static str, help: &'static dyn Help) {
    table(env).insert(name, help);
}

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
    let Some(&help) = table(env).get(operand.value.as_str()) else {
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
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stdout;

    fn env() -> (Env<Rc<Concurrent<VirtualSystem>>>, VirtualSystem) {
        let system = VirtualSystem::new();
        let env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        (env, system)
    }

    struct DummyHelp;

    impl Help for DummyHelp {
        fn summary(&self) -> &str {
            "dummy"
        }

        fn message(&self) -> String {
            "dummy help\n".to_string()
        }
    }

    #[test]
    fn prints_help_of_operand() {
        let (mut env, system) = env();

        let result = main(&mut env, Field::dummies(["pwd"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, crate::pwd::HELP.message())
        });
    }
    #[test]
    fn prints_registered_help() {
        let (mut env, system) = env();
        register(&mut env, "dummy", &DummyHelp);

        let result = main(&mut env, Field::dummies(["dummy"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "dummy help\n"));
    }

    #[test]
    fn registered_help_replaces_existing_one() {
        let (mut env, system) = env();
        register(&mut env, "pwd", &DummyHelp);

        let result = main(&mut env, Field::dummies(["pwd"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::default());
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "dummy help\n"));
    }
}
