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

//! Semantics of subshell compound commands

use crate::Command;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax::List;

/// Executes a subshell command
pub async fn execute(env: &mut Env, list: &List) -> Result {
    let list = list.clone(); // TODO Avoid cloning the entire list
    let result = env
        .run_in_subshell(|sub_env| Box::pin(async move { list.execute(sub_env).await }))
        .await;
    match result {
        Ok(exit_status) => {
            env.exit_status = exit_status;
            Continue(())
        }
        Err(_) => todo!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use std::str::from_utf8;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::FileBody;
    use yash_syntax::syntax::CompoundCommand;

    #[test]
    fn subshell_preserves_current_environment() {
        in_virtual_system(|mut env, _pid, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let command: CompoundCommand = "(foo=bar; echo $foo; return -n 123)".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus(123));

            assert_eq!(env.variables.get("foo"), None);

            let stdout = state.borrow().file_system.get("/dev/stdout").unwrap();
            let stdout = stdout.borrow();
            assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
                assert_eq!(from_utf8(content), Ok("bar\n"));
            });
        })
    }
}
