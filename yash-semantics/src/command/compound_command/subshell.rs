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

use crate::command::Command;
use crate::trap::run_exit_trap;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::io::print_error;
use yash_env::job::Job;
use yash_env::job::ProcessState;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::subshell::JobControl;
use yash_env::subshell::Subshell;
use yash_env::Env;
use yash_syntax::source::Location;
use yash_syntax::syntax::List;

/// Executes a subshell command
pub async fn execute(env: &mut Env, body: Rc<List>, location: &Location) -> Result {
    let body_2 = Rc::clone(&body);
    let subshell = Subshell::new(|sub_env, _job_control| Box::pin(subshell_main(sub_env, body_2)));
    let subshell = subshell.job_control(JobControl::Foreground);
    match subshell.start_and_wait(env).await {
        Ok((pid, state)) => {
            if let ProcessState::Stopped(_) = state {
                let mut job = Job::new(pid);
                job.job_controlled = true;
                job.state = state;
                job.name = body.to_string();
                env.jobs.add(job);
            }

            env.exit_status = state.try_into().unwrap();
            env.apply_errexit()
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

/// Executes the content of the shell.
async fn subshell_main(env: &mut Env, body: Rc<List>) -> Result {
    let result = body.execute(env).await;
    env.apply_result(result);

    run_exit_trap(env).await;

    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use crate::tests::stub_tty;
    use crate::tests::suspend_builtin;
    use futures_util::FutureExt;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use yash_env::job::ProcessState;
    use yash_env::option::Option::{ErrExit, Monitor};
    use yash_env::option::State::On;
    use yash_env::trap::Signal;
    use yash_env::VirtualSystem;
    use yash_syntax::syntax::CompoundCommand;

    #[test]
    fn subshell_preserves_current_environment() {
        in_virtual_system(|mut env, state| async move {
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
    fn divert_in_subshell() {
        fn exit_builtin(
            _env: &mut Env,
            _args: Vec<yash_env::semantics::Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                yash_env::builtin::Result::with_exit_status_and_divert(
                    ExitStatus::SUCCESS,
                    Break(Divert::Exit(Some(ExitStatus(21)))),
                )
            })
        }

        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert(
                "exit",
                yash_env::builtin::Builtin {
                    r#type: yash_env::builtin::Type::Special,
                    execute: exit_builtin,
                },
            );

            let command: CompoundCommand = "(exit)".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus(21));
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

    #[test]
    fn errexit_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            env.options.set(ErrExit, On);
            let command: CompoundCommand = "(return -n 42)".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Break(Divert::Exit(None)));
            assert_eq!(env.exit_status, ExitStatus(42));
        })
    }

    #[test]
    fn job_controlled_subshell() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());
            env.options.set(Monitor, On);
            stub_tty(&state);

            let command: CompoundCommand = "(return -n 12)".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus(12));

            let state = state.borrow();
            let (&pid, process) = state.processes.last_key_value().unwrap();
            assert_ne!(pid, env.main_pid);
            assert_ne!(process.pgid(), env.main_pgid);
            assert_eq!(process.state(), ProcessState::Exited(ExitStatus(12)));

            assert_eq!(env.jobs.len(), 0);
        })
    }

    #[test]
    fn job_controlled_suspended_subshell_in_job_set() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("suspend", suspend_builtin());
            env.options.set(Monitor, On);
            stub_tty(&state);

            let command: CompoundCommand = "(suspend foo)".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::from(Signal::SIGSTOP));

            let state = state.borrow();
            let (&pid, process) = state.processes.last_key_value().unwrap();
            assert_ne!(pid, env.main_pid);
            assert_ne!(process.pgid(), env.main_pgid);
            assert_eq!(process.state(), ProcessState::Stopped(Signal::SIGSTOP));

            assert_eq!(env.jobs.len(), 1);
            let job = env.jobs.iter().next().unwrap().1;
            assert_eq!(job.pid, pid);
            assert!(job.job_controlled);
            assert_eq!(job.state, ProcessState::Stopped(Signal::SIGSTOP));
            assert!(job.state_changed);
            assert_eq!(job.name, "suspend foo");
        })
    }

    #[test]
    fn exit_trap() {
        fn trap_builtin(
            env: &mut Env,
            _args: Vec<yash_env::semantics::Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                env.traps
                    .set_action(
                        &mut env.system,
                        yash_env::trap::Condition::Exit,
                        yash_env::trap::Action::Command("echo exiting".into()),
                        Location::dummy(""),
                        false,
                    )
                    .unwrap();
                yash_env::builtin::Result::default()
            })
        }

        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert(
                "trap",
                yash_env::builtin::Builtin {
                    r#type: yash_env::builtin::Type::Special,
                    execute: trap_builtin,
                },
            );

            let command: CompoundCommand = "(trap)".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            assert_stdout(&state, |stdout| assert_eq!(stdout, "exiting\n"));
        })
    }
}
