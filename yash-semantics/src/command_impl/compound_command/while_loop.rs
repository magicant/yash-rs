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

//! Execution of the while loop

use crate::Command;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::{ExitStatus, Result};
use yash_env::Env;
use yash_syntax::syntax::List;

/// Executes the while loop.
pub async fn execute_while(env: &mut Env, condition: &List, body: &List) -> Result {
    // TODO Handle break and continue
    condition.execute(env).await?;
    if env.exit_status == ExitStatus::SUCCESS {
        // TODO Handle break and continue
        body.execute(env).await?;
    } else {
        env.exit_status = ExitStatus::SUCCESS;
    }
    // TODO loop
    Continue(())
}

/// Executes the until loop.
pub async fn execute_until(_env: &mut Env, _condition: &List, _body: &List) -> Result {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use crate::Command;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::VirtualSystem;
    use yash_syntax::syntax::CompoundCommand;

    #[test]
    fn zero_round_while_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(15);
        let command = "while echo $?; return -n 1; do echo unreached; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "15\n"));
    }

    #[test]
    fn one_round_while_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let command = "while return -n $?0; do echo body; return -n 7; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(7));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "body\n"));
    }

    #[test]
    fn return_from_while_condition() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let command: CompoundCommand = "while return 36; echo X; do echo Y; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(36));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_while_body() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let command: CompoundCommand = "while echo A; do return 42; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(42));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "A\n"));
    }
}
