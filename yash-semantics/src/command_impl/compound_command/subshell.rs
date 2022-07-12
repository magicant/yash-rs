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
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::io::print_error;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::source::Location;
use yash_syntax::syntax::List;

/// Executes a subshell command
pub async fn execute(env: &mut Env, body: Rc<List>, location: &Location) -> Result {
    let result = env
        .run_in_subshell(|sub_env| Box::pin(async move { body.execute(sub_env).await }))
        .await;
    match result {
        Ok(exit_status) => {
            env.exit_status = exit_status;
            Continue(())
        }
        Err(errno) => {
            print_error(
                env,
                "cannot start subshell".into(),
                errno.desc().into(),
                location,
            )
            .await;
            Break(Divert::Interrupt(Some(ExitStatus::ERROR)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
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
            assert_stdout(&state, |stdout| assert_eq!(stdout, "bar\n"));
        })
    }

    #[test]
    fn error_starting_subshell() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let command: CompoundCommand = "(foo=bar; echo $foo; return -n 123)".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }
}