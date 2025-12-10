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

//! Execution of the if command

use super::evaluate_condition;
use crate::command::Command;
use std::ops::ControlFlow::Continue;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::system::System;
use yash_syntax::syntax::ElifThen;
use yash_syntax::syntax::List;

/// Executes the if command.
pub async fn execute<S: System + 'static>(
    env: &mut Env<S>,
    condition: &List,
    body: &List,
    elifs: &[ElifThen],
    r#else: &Option<List>,
) -> Result {
    if evaluate_condition(env, condition).await? {
        return body.execute(env).await;
    }
    for ElifThen { condition, body } in elifs {
        if evaluate_condition(env, condition).await? {
            return body.execute(env).await;
        }
    }
    if let Some(r#else) = r#else {
        r#else.execute(env).await
    } else {
        env.exit_status = ExitStatus::SUCCESS;
        Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::semantics::Divert;
    use yash_env::system::r#virtual::SystemState;
    use yash_env_test_helper::assert_stdout;
    use yash_syntax::syntax::CompoundCommand;

    fn fixture() -> (Env<VirtualSystem>, Rc<RefCell<SystemState>>) {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        (env, state)
    }

    #[test]
    fn true_condition_without_else() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(15);
        let command = "if echo $?; then return -n 123; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(123));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "15\n"));
    }

    #[test]
    fn true_condition_with_else() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(15);
        let command = "if echo $?; then return -n 123; else echo not reached; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(123));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "15\n"));
    }

    #[test]
    fn false_condition_without_else() {
        let (mut env, state) = fixture();
        let command = "if return -n 1; then echo not reached; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn false_condition_with_else() {
        let (mut env, state) = fixture();
        let command = "if return -n 29; then echo not reached; else echo $?; return -n 43; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(43));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "29\n"));
    }

    #[test]
    fn true_first_elif_condition() {
        let (mut env, state) = fixture();
        let command = "if return -n 97; then echo not reached 1
        elif echo $?; then return -n 61
        elif echo not reached 3; then echo not reached 4; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(61));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "97\n"));
    }

    #[test]
    fn true_second_elif_condition() {
        let (mut env, state) = fixture();
        let command = "if return -n 10; then echo not reached 1
        elif return -n 20; then echo not reached 2
        elif echo $?; then return -n 9
        else echo not reached 3; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(9));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "20\n"));
    }

    #[test]
    fn false_elif_conditions_without_else() {
        let (mut env, state) = fixture();
        let command = "if return -n 1; then echo not reached 1
        elif return -n 2; then echo not reached 2
        elif return -n 3; then echo not reached 3; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn false_elif_conditions_with_else() {
        let (mut env, state) = fixture();
        let command = "if return -n 101; then echo not reached 1
        elif return -n 102; then echo not reached 2
        elif return -n 103; then echo not reached 3
        elif return -n 104; then echo not reached 4
        else echo $?; return -n 200; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(200));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "104\n"));
    }

    #[test]
    fn return_from_condition() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(15);
        let command = "if return -n 7; return 42; then echo not reached; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(42)))));
        assert_eq!(env.exit_status, ExitStatus(7));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_body() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(15);
        let command = "if return -n 0; then return -n 9; return 73; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(73)))));
        assert_eq!(env.exit_status, ExitStatus(9));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_elif_condition() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(15);
        let command = "if return -n 2; then echo not reached 1
        elif return 52; then echo not reached 2; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(52)))));
        assert_eq!(env.exit_status, ExitStatus(2));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_elif_body() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(15);
        let command = "if return -n 2; then echo not reached 1
        elif return -n 0; then return -n 6; return 47; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(47)))));
        assert_eq!(env.exit_status, ExitStatus(6));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_else() {
        let (mut env, state) = fixture();
        let command = "if return -n 13; then echo not reached; else return 17; fi";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(17)))));
        assert_eq!(env.exit_status, ExitStatus(13));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }
}
