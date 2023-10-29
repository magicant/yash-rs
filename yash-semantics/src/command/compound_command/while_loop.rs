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

//! Execution of the while and until loop

use crate::command::Command;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::semantics::Divert;
use yash_env::semantics::{ExitStatus, Result};
use yash_env::stack::Frame;
use yash_env::Env;
use yash_syntax::syntax::List;

/// Execution context for loops
struct Loop<'a> {
    env: &'a mut Env,
    condition_command: &'a List,
    expected_condition: bool,
    body: &'a List,
    exit_status: ExitStatus,
}

impl Loop<'_> {
    async fn iterate(&mut self) -> Result {
        while super::evaluate_condition(self.env, self.condition_command).await?
            == self.expected_condition
        {
            self.body.execute(self.env).await?;
            self.exit_status = self.env.exit_status;
        }
        Continue(())
    }

    async fn execute(&mut self) -> Result {
        loop {
            match self.iterate().await {
                Break(Divert::Break { count: 0 }) => {
                    self.exit_status = self.env.exit_status;
                    return Continue(());
                }
                Break(Divert::Break { count }) => return Break(Divert::Break { count: count - 1 }),
                Break(Divert::Continue { count: 0 }) => continue,
                Break(Divert::Continue { count }) => {
                    return Break(Divert::Continue { count: count - 1 })
                }
                other => return other,
            }
        }
    }
}

async fn execute_common(
    env: &mut Env,
    condition_command: &List,
    expected_condition: bool,
    body: &List,
) -> Result {
    let env = &mut env.push_frame(Frame::Loop);
    let mut l = Loop {
        env,
        condition_command,
        expected_condition,
        body,
        exit_status: ExitStatus::default(),
    };
    l.execute().await?;
    env.exit_status = l.exit_status;
    Continue(())
}

/// Executes the while loop.
pub async fn execute_while(env: &mut Env, condition: &List, body: &List) -> Result {
    execute_common(env, condition, true, body).await
}

/// Executes the until loop.
pub async fn execute_until(env: &mut Env, condition: &List, body: &List) -> Result {
    execute_common(env, condition, false, body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::break_builtin;
    use crate::tests::continue_builtin;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use yash_env::builtin::Builtin;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
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
            Some(Value::scalar("3"))
        );
    }

    #[test]
    fn return_from_while_condition() {
        let (mut env, state) = fixture();
        let command = "while return -n 1; return 36; echo X; do echo Y; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(36)))));
        assert_eq!(env.exit_status, ExitStatus(1));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_while_body() {
        let (mut env, state) = fixture();
        let command = "while echo A; do return -n 2; return 42; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(42)))));
        assert_eq!(env.exit_status, ExitStatus(2));
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
                Default::default()
            })
        }
        let (mut env, _state) = fixture();
        let r#type = yash_env::builtin::Type::Mandatory;
        env.builtins.insert("check", Builtin { r#type, execute });
        let command: CompoundCommand = "while check; do check; return; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(None)));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.stack[..], []);
    }

    #[test]
    fn break_while_loop_condition() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "while break; do echo 1; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn break_while_loop_body() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "while echo 1; do break; echo 2; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1\n"));
    }

    #[test]
    fn exit_status_of_broken_while_loop() {
        let mut env = Env::new_virtual();
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand =
            "while return -n $((i)) || break; do i=1; return -n 100; done"
                .parse()
                .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        // It is POSIXly unclear what the exit status of the above command
        // should be. Our implementation returns that of the break built-in,
        // which is SUCCESS, rather than that of the previously executed loop
        // body. Returning the result of the previous iteration would not be
        // very sensible if the break built-in fired in the middle of the body.
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn break_outer_loop_of_while() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "while break $n; do echo 1; done".parse().unwrap();

        for n in 2..5 {
            env.exit_status = ExitStatus(123);
            env.variables
                .get_or_new("n".into(), Scope::Global)
                .assign(n.to_string().into(), None)
                .unwrap();

            let result = command.execute(&mut env).now_or_never().unwrap();
            assert_eq!(result, Break(Divert::Break { count: n - 2 }));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        }
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn continue_while_loop_condition() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "while return -n $(((i+=1)>3)) && continue; do echo X; done"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn continue_while_loop_body() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand =
            "while return -n $(((i+=1)>3)); do echo +$i; continue; echo -$i; done"
                .parse()
                .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "+1\n+2\n+3\n"));
    }

    #[test]
    fn exit_status_of_continued_while_loop() {
        let mut env = Env::new_virtual();
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "while
            case $count in
                ('') count=1 ;;
                (1) count=2; continue ;;
                (*) return -n 1 ;;
            esac
        do
            return -n 100
        done"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(100));
    }

    #[test]
    fn continue_outer_loop_of_while() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "while continue $n; do echo 1; done".parse().unwrap();

        for n in 2..5 {
            env.exit_status = ExitStatus(123);
            env.variables
                .get_or_new("n".into(), Scope::Global)
                .assign(n.to_string().into(), None)
                .unwrap();

            let result = command.execute(&mut env).now_or_never().unwrap();
            assert_eq!(result, Break(Divert::Continue { count: n - 2 }));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        }
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
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
            Some(Value::scalar("3"))
        );
    }

    #[test]
    fn return_from_until_condition() {
        let (mut env, state) = fixture();
        let command = "until return -n 5; return 12; echo X; do echo Y; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(12)))));
        assert_eq!(env.exit_status, ExitStatus(5));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn return_from_until_body() {
        let (mut env, _state) = fixture();
        let command: CompoundCommand = "until return -n 9; do return 35; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(35)))));
        assert_eq!(env.exit_status, ExitStatus(9));
    }

    #[test]
    fn stack_frame_in_until_loop() {
        fn execute(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_eq!(env.stack[0], Frame::Loop);
                Default::default()
            })
        }
        let (mut env, _state) = fixture();
        let r#type = yash_env::builtin::Type::Mandatory;
        env.builtins.insert("check", Builtin { r#type, execute });
        let command: CompoundCommand = "until ! check; do check; return; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(None)));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.stack[..], []);
    }

    #[test]
    fn break_until_loop_condition() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "until ! break; do echo 1; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn break_until_loop_body() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "until ! echo 1; do break; echo 2; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1\n"));
    }

    #[test]
    fn exit_status_of_broken_until_loop() {
        let mut env = Env::new_virtual();
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand =
            "until return -n $((i+1)) && break; do i=-1; return -n 100; done"
                .parse()
                .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        // See also exit_status_of_broken_while_loop
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn break_outer_loop_of_until() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "until ! break $n; do echo 1; done".parse().unwrap();

        for n in 2..5 {
            env.exit_status = ExitStatus(123);
            env.variables
                .get_or_new("n".into(), Scope::Global)
                .assign(n.to_string().into(), None)
                .unwrap();

            let result = command.execute(&mut env).now_or_never().unwrap();
            assert_eq!(result, Break(Divert::Break { count: n - 2 }));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        }
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn continue_until_loop_condition() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "until return -n $(((i+=1)<3)) || continue; do echo X; done"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn continue_until_loop_body() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand =
            "until return -n $(((i+=1)<3)); do echo +$i; continue; echo -$i; done"
                .parse()
                .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "+1\n+2\n"));
    }

    #[test]
    fn exit_status_of_continued_until_loop() {
        let mut env = Env::new_virtual();
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "until
            case $count in
                ('') count=1; return -n 1 ;;
                (1) count=2; continue ;;
                (*) return -n 0 ;;
            esac
        do
            return -n 100
        done"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(100));
    }

    #[test]
    fn continue_outer_loop_of_until() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "until continue $n; do echo 1; done".parse().unwrap();

        for n in 2..5 {
            env.exit_status = ExitStatus(123);
            env.variables
                .get_or_new("n".into(), Scope::Global)
                .assign(n.to_string().into(), None)
                .unwrap();

            let result = command.execute(&mut env).now_or_never().unwrap();
            assert_eq!(result, Break(Divert::Continue { count: n - 2 }));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        }
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }
}
