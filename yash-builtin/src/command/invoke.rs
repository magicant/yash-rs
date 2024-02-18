// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Command invoking semantics

use crate::common::report_failure;

use super::identify::NotFound;
use super::search::SearchEnv;
use super::Invoke;
use yash_env::semantics::ExitStatus;
use yash_env::Env;
use yash_semantics::command_search::search;

impl Invoke {
    /// Execute the command
    pub async fn execute(&self, env: &mut Env) -> crate::Result {
        let Some(name) = self.fields.first() else {
            return crate::Result::default();
        };

        let params = &self.search;
        let search_env = &mut SearchEnv { env, params };
        let Some(target) = search(search_env, &name.value) else {
            let mut result = report_failure(env, &NotFound { name }).await;
            result.set_exit_status(ExitStatus::NOT_FOUND);
            return result;
        };

        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::super::Search;
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use enumset::EnumSet;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::Special;
    use yash_env::semantics::Field;
    use yash_env::VirtualSystem;

    #[test]
    fn empty_command_invocation() {
        let mut env = Env::new_virtual();
        let invoke = Invoke::default();
        let result = invoke.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, crate::Result::default());
    }

    #[test]
    fn command_not_found() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert(
            "foo",
            Builtin {
                r#type: Special,
                execute: |_, _| unreachable!(),
            },
        );
        let invoke = Invoke {
            fields: Field::dummies(["foo"]),
            search: Search {
                standard_path: false,
                categories: EnumSet::empty(),
            },
        };

        let result = invoke.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::NOT_FOUND);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("not found"), "stderr: {stderr:?}");
        });
    }
}
