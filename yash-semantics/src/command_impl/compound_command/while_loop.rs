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
use yash_env::stack::Frame;
use yash_env::Env;
use yash_syntax::syntax::List;

async fn execute_condition(env: &mut Env, condition: &List) -> Result<bool> {
    condition.execute(env).await?;
    Continue(env.exit_status == ExitStatus::SUCCESS)
}

async fn execute_loop(
    env: &mut Env,
    condition_command: &List,
    expected_condition: bool,
    body: &List,
) -> Result {
    let env = &mut env.push_frame(Frame::Loop);

    let mut exit_status = ExitStatus::SUCCESS;
    // TODO Handle break and continue
    while execute_condition(env, condition_command).await? == expected_condition {
        body.execute(env).await?;
        exit_status = env.exit_status;
    }
    env.exit_status = exit_status;
    Continue(())
}

/// Executes the while loop.
pub async fn execute_while(env: &mut Env, condition: &List, body: &List) -> Result {
    execute_loop(env, condition, true, body).await
}

/// Executes the until loop.
pub async fn execute_until(env: &mut Env, condition: &List, body: &List) -> Result {
    execute_loop(env, condition, false, body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use crate::Command;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::future::Future;
    use std::ops::ControlFlow::Break;
    use std::pin::Pin;
    use std::rc::Rc;
    use yash_env::builtin::Builtin;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::variable::Value::Scalar;
    use yash_env::VirtualSystem;
    use yash_syntax::syntax::CompoundCommand;

    fn fixture() -> (Env, Rc<RefCell<SystemState>>) {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        (env, state)
    }

    #[test]
    fn zero_round_while_loop() {
        let (mut env, state) = fixture();
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
        let (mut env, state) = fixture();
        let command = "while return -n $?0; do echo body; return -n 7; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(7));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "body\n"));
    }

    #[test]
    fn three_round_while_loop() {
        let (mut env, _state) = fixture();
        let command = "while return -n $((a>=3)); do a=$((a+1)); return -n $((a*10)); done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(30));
        assert_eq!(
            env.variables.get("a").unwrap().value,
            Scalar("3".to_string())
        );
    }

    #[test]
    fn return_from_while_condition() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "while return 36; echo X; do echo Y; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(36));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_while_body() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "while echo A; do return 42; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(42));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "A\n"));
    }

    #[test]
    fn stack_frame_in_while_loop() {
        fn execute(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_eq!(env.stack[0], Frame::Loop);
                (ExitStatus::SUCCESS, Continue(()))
            })
        }
        let (mut env, _state) = fixture();
        let r#type = yash_env::builtin::Type::Intrinsic;
        env.builtins.insert("check", Builtin { r#type, execute });
        let command: CompoundCommand = "while check; do check; return; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.stack[..], []);
    }

    #[test]
    fn zero_round_until_loop() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(17);
        let command = "until echo $?; return -n 0; do echo unreached; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "17\n"));
    }

    #[test]
    fn one_round_until_loop() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(10);
        let command = "until return -n $(($?/10)); do echo body; return -n 7; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(7));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "body\n"));
    }

    #[test]
    fn three_round_until_loop() {
        let (mut env, _state) = fixture();
        let command = "until return -n $((a<3)); do a=$((a+1)); return -n $((a*10)); done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(30));
        assert_eq!(
            env.variables.get("a").unwrap().value,
            Scalar("3".to_string())
        );
    }

    #[test]
    fn return_from_until_condition() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "until return 12; echo X; do echo Y; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(12));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_until_body() {
        let (mut env, _state) = fixture();
        let command: CompoundCommand = "until return -n 9; do return 35; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(35));
    }

    #[test]
    fn stack_frame_in_until_loop() {
        fn execute(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_eq!(env.stack[0], Frame::Loop);
                (ExitStatus::SUCCESS, Continue(()))
            })
        }
        let (mut env, _state) = fixture();
        let r#type = yash_env::builtin::Type::Intrinsic;
        env.builtins.insert("check", Builtin { r#type, execute });
        let command: CompoundCommand = "until ! check; do check; return; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.stack[..], []);
    }
}
