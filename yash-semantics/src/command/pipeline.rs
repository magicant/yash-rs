// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Implementation of pipeline semantics.

use super::Command;
use crate::job::add_job_if_suspended;
use crate::trap::run_exit_trap;
use enumset::EnumSet;
use itertools::Itertools;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::Env;
use yash_env::System;
use yash_env::io::Fd;
use yash_env::job::Pid;
use yash_env::option::Option::{Exec, Interactive};
use yash_env::option::State::Off;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::subshell::JobControl;
use yash_env::subshell::Subshell;
use yash_env::system::Errno;
use yash_syntax::syntax;

/// Executes the pipeline.
///
/// # Executing commands
///
/// If this pipeline contains one command, it is executed in the current shell
/// execution environment.
///
/// If the pipeline has more than one command, all the commands are executed
/// concurrently. Every command is executed in a new subshell. The standard
/// output of a command is connected to the standard input of the next command
/// via a pipe, except for the standard output of the last command and the
/// standard input of the first command, which are not modified.
///
/// If the pipeline has no command, it is a no-op.
///
/// # Exit status
///
/// The exit status of the pipeline is that of the last command (or zero if no
/// command). If the pipeline starts with an `!`, the exit status is inverted:
/// zero becomes one, and non-zero becomes zero.
///
/// In POSIX, the expected exit status is unclear when an inverted pipeline
/// performs a jump as in `! return 42`. The behavior disagrees among existing
/// shells. This implementation does not invert the exit status when the return
/// value is `Err(Divert::...)`.
///
/// # `noexec` option
///
/// If the [`Exec`] and [`Interactive`] options are [`Off`] in `env.options`,
/// the entire execution of the pipeline is skipped. (The `noexec` option is
/// ignored if the shell is interactive, otherwise you cannot exit the shell
/// in any way if the `ignoreeof` option is set.)
///
/// # Stack
///
/// if `self.negation` is true, [`Frame::Condition`] is pushed to the
/// environment's stack while the pipeline is executed.
impl Command for syntax::Pipeline {
    async fn execute(&self, env: &mut Env) -> Result {
        if env.options.get(Exec) == Off && env.options.get(Interactive) == Off {
            return Continue(());
        }

        if !self.negation {
            return execute_commands_in_pipeline(env, &self.commands).await;
        }

        let mut env = env.push_frame(Frame::Condition);
        execute_commands_in_pipeline(&mut env, &self.commands).await?;
        env.exit_status = if env.exit_status.is_successful() {
            ExitStatus::FAILURE
        } else {
            ExitStatus::SUCCESS
        };
        Continue(())
    }
}

async fn execute_commands_in_pipeline(env: &mut Env, commands: &[Rc<syntax::Command>]) -> Result {
    match commands.len() {
        0 => {
            env.exit_status = ExitStatus::SUCCESS;
            Continue(())
        }

        1 => commands[0].execute(env).await,

        _ => {
            if env.controls_jobs() {
                execute_job_controlled_pipeline(env, commands).await?
            } else {
                execute_multi_command_pipeline(env, commands).await?
            }
            env.apply_errexit()
        }
    }
}

async fn execute_job_controlled_pipeline(
    env: &mut Env,
    commands: &[Rc<syntax::Command>],
) -> Result {
    let commands_2 = commands.to_vec();
    let subshell = Subshell::new(|sub_env, _job_control| {
        Box::pin(async move {
            let result = execute_multi_command_pipeline(sub_env, &commands_2).await;
            sub_env.apply_result(result);
            run_exit_trap(sub_env).await;
        })
    })
    .job_control(JobControl::Foreground);

    match subshell.start_and_wait(env).await {
        Ok((pid, result)) => {
            env.exit_status = add_job_if_suspended(env, pid, result, || to_job_name(commands))?;
            Continue(())
        }
        Err(errno) => {
            // TODO print error location using yash_env::io::print_error
            let message = format!("cannot start a subshell in the pipeline: {errno}\n");
            env.system.print_error(&message).await;
            Break(Divert::Interrupt(Some(ExitStatus::NOEXEC)))
        }
    }
}

fn to_job_name(commands: &[Rc<syntax::Command>]) -> String {
    commands
        .iter()
        .format_with(" | ", |cmd, f| f(&format_args!("{cmd}")))
        .to_string()
}

async fn execute_multi_command_pipeline(env: &mut Env, commands: &[Rc<syntax::Command>]) -> Result {
    // Start commands
    let mut commands = commands.iter().cloned();
    let mut pipes = PipeSet::new();
    let mut pids = Vec::new();
    while let Some(command) = commands.next() {
        let has_next = commands.len() > 0; // TODO ExactSizeIterator::is_empty
        shift_or_fail(env, &mut pipes, has_next).await?;

        let pipes = pipes;
        let subshell = Subshell::new(move |env, _job_control| {
            Box::pin(async move {
                let result = connect_pipe_and_execute_command(env, pipes, command).await;
                env.apply_result(result);
                run_exit_trap(env).await;
            })
        });
        let start_result = subshell.start(env).await;
        pids.push(pid_or_fail(env, start_result).await?);
    }

    shift_or_fail(env, &mut pipes, false).await?;

    // Await the last command
    for pid in pids {
        // TODO Report if the child was signaled and the shell is interactive
        env.exit_status = env
            .wait_for_subshell_to_finish(pid)
            .await
            .expect("cannot receive exit status of child process")
            .1;
    }
    Continue(())
}

async fn shift_or_fail(env: &mut Env, pipes: &mut PipeSet, has_next: bool) -> Result {
    match pipes.shift(env, has_next) {
        Ok(()) => Continue(()),
        Err(errno) => {
            // TODO print error location using yash_env::io::print_error
            let message = format!("cannot connect pipes in the pipeline: {errno}\n");
            env.system.print_error(&message).await;
            Break(Divert::Interrupt(Some(ExitStatus::NOEXEC)))
        }
    }
}

async fn connect_pipe_and_execute_command(
    env: &mut Env,
    pipes: PipeSet,
    command: Rc<syntax::Command>,
) -> Result {
    match pipes.move_to_stdin_stdout(env) {
        Ok(()) => (),
        Err(errno) => {
            // TODO print error location using yash_env::io::print_error
            let message = format!("cannot connect pipes in the pipeline: {errno}\n");
            env.system.print_error(&message).await;
            return Break(Divert::Interrupt(Some(ExitStatus::NOEXEC)));
        }
    }

    command.execute(env).await
}

async fn pid_or_fail(
    env: &mut Env,
    start_result: std::result::Result<(Pid, Option<JobControl>), Errno>,
) -> Result<Pid> {
    match start_result {
        Ok((pid, job_control)) => {
            debug_assert_eq!(job_control, None);
            Continue(pid)
        }
        Err(errno) => {
            // TODO print error location using yash_env::io::print_error
            env.system
                .print_error(&format!(
                    "cannot start a subshell in the pipeline: {errno}\n"
                ))
                .await;
            Break(Divert::Interrupt(Some(ExitStatus::NOEXEC)))
        }
    }
}

/// Set of pipe file descriptors that connect commands.
#[derive(Clone, Copy, Default)]
struct PipeSet {
    read_previous: Option<Fd>,
    /// Reader and writer to the next command.
    next: Option<(Fd, Fd)>,
}

impl PipeSet {
    fn new() -> Self {
        Self::default()
    }

    /// Updates the pipe set for the next command.
    ///
    /// Closes FDs that are no longer necessary and opens a new pipe if there is
    /// a next command.
    fn shift(&mut self, env: &mut Env, has_next: bool) -> std::result::Result<(), Errno> {
        if let Some(fd) = self.read_previous {
            let _ = env.system.close(fd);
        }

        if let Some((reader, writer)) = self.next {
            let _ = env.system.close(writer);
            self.read_previous = Some(reader);
        } else {
            self.read_previous = None;
        }

        self.next = None;
        if has_next {
            self.next = Some(env.system.pipe()?);
        }

        Ok(())
    }

    /// Moves the pipe FDs to stdin/stdout and closes the FDs that are no longer
    /// necessary.
    fn move_to_stdin_stdout(mut self, env: &mut Env) -> std::result::Result<(), Errno> {
        if let Some((reader, writer)) = self.next {
            assert_ne!(reader, writer);
            assert_ne!(self.read_previous, Some(reader));
            assert_ne!(self.read_previous, Some(writer));

            env.system.close(reader)?;
            if writer != Fd::STDOUT {
                if self.read_previous == Some(Fd::STDOUT) {
                    self.read_previous =
                        Some(env.system.dup(Fd::STDOUT, Fd(0), EnumSet::empty())?);
                }
                env.system.dup2(writer, Fd::STDOUT)?;
                env.system.close(writer)?;
            }
        }
        if let Some(reader) = self.read_previous {
            if reader != Fd::STDIN {
                env.system.dup2(reader, Fd::STDIN)?;
                env.system.close(reader)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::cat_builtin;
    use crate::tests::return_builtin;
    use crate::tests::suspend_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::pin::Pin;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::Special;
    use yash_env::job::ProcessResult;
    use yash_env::job::ProcessState;
    use yash_env::option::Option::ErrExit;
    use yash_env::option::Option::Monitor;
    use yash_env::option::State::On;
    use yash_env::semantics::Field;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::SIGSTOP;
    use yash_env_test_helper::assert_stdout;
    use yash_env_test_helper::in_virtual_system;
    use yash_env_test_helper::stub_tty;

    #[test]
    fn empty_pipeline() {
        let mut env = Env::new_virtual();
        let pipeline = syntax::Pipeline {
            commands: vec![],
            negation: false,
        };
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(0));
    }

    #[test]
    fn single_command_pipeline_returns_exit_status_intact_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "return -n 93".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(93));
    }

    #[test]
    fn single_command_pipeline_returns_exit_status_intact_with_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(17);
        let pipeline: syntax::Pipeline = "return 37".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(37)))));
        assert_eq!(env.exit_status, ExitStatus(17));
    }

    #[test]
    fn multi_command_pipeline_returns_last_command_exit_status() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let pipeline: syntax::Pipeline = "return -n 10 | return -n 20".parse().unwrap();
            let result = pipeline.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus(20));
        });
    }

    #[test]
    fn multi_command_pipeline_waits_for_all_child_commands() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());
            let pipeline: syntax::Pipeline =
                "return -n 1 | return -n 2 | return -n 3".parse().unwrap();
            _ = pipeline.execute(&mut env).await;

            // Only the original process remains.
            for (pid, process) in &state.borrow().processes {
                if *pid == env.main_pid {
                    assert_eq!(process.state(), ProcessState::Running);
                } else {
                    assert_matches!(
                        process.state(),
                        ProcessState::Halted(ProcessResult::Exited(_))
                    );
                }
            }
        });
    }

    #[test]
    fn multi_command_pipeline_does_not_wait_for_unrelated_child() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());

            let list: syntax::List = "return -n 7&".parse().unwrap();
            _ = list.execute(&mut env).await;
            let async_pid = {
                let state = state.borrow();
                let mut iter = state.processes.keys();
                assert_eq!(iter.next(), Some(&env.main_pid));
                let async_pid = *iter.next().unwrap();
                assert_eq!(iter.next(), None);
                async_pid
            };

            let pipeline: syntax::Pipeline =
                "return -n 1 | return -n 2 | return -n 3".parse().unwrap();
            _ = pipeline.execute(&mut env).await;

            let state = state.borrow();
            let process = &state.processes[&async_pid];
            assert_eq!(process.state(), ProcessState::exited(7));
            assert!(process.state_has_changed());
        });
    }

    #[test]
    fn pipe_connects_commands_in_pipeline() {
        in_virtual_system(|mut env, state| async move {
            {
                let file = state.borrow().file_system.get("/dev/stdin").unwrap();
                let mut file = file.borrow_mut();
                file.body = FileBody::new(*b"ok\n");
            }

            env.builtins.insert("cat", cat_builtin());

            let pipeline: syntax::Pipeline = "cat | cat | cat".parse().unwrap();
            let result = pipeline.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            assert_stdout(&state, |stdout| assert_eq!(stdout, "ok\n"));
        });
    }

    #[test]
    fn pipeline_leaves_no_pipe_fds_leftover() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("cat", cat_builtin());
            let pipeline: syntax::Pipeline = "cat | cat".parse().unwrap();
            let _ = pipeline.execute(&mut env).await;

            let state = state.borrow();
            let fds = state.processes[&env.main_pid].fds();
            for fd in 3..10 {
                assert!(!fds.contains_key(&Fd(fd)), "fd={fd}");
            }
        });
    }

    #[test]
    fn inverting_exit_status_to_0_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "! return -n 42".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(0));
    }

    #[test]
    fn inverting_exit_status_to_1_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "! return -n 0".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(1));
    }

    #[test]
    fn not_inverting_exit_status_with_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.exit_status = ExitStatus(3);
        let pipeline: syntax::Pipeline = "! return 15".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(15)))));
        assert_eq!(env.exit_status, ExitStatus(3));
    }

    #[test]
    fn noexec_option() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.options.set(Exec, Off);
        let pipeline: syntax::Pipeline = "return -n 93".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn noexec_option_interactive() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.options.set(Exec, Off);
        env.options.set(Interactive, On);
        let pipeline: syntax::Pipeline = "return -n 93".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(93));
    }

    #[test]
    fn errexit_option() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            env.options.set(ErrExit, On);

            let pipeline: syntax::Pipeline = "return -n 0 | return -n 93".parse().unwrap();
            let result = pipeline.execute(&mut env).await;

            assert_eq!(result, Break(Divert::Exit(None)));
            assert_eq!(env.exit_status, ExitStatus(93));
        });
    }

    #[test]
    fn stack_without_inversion() {
        fn stub_builtin(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert!(!env.stack.contains(&Frame::Condition), "{:?}", env.stack);
                Default::default()
            })
        }

        let mut env = Env::new_virtual();
        env.builtins
            .insert("foo", Builtin::new(Special, stub_builtin));
        let pipeline: syntax::Pipeline = "foo".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
    }

    #[test]
    fn stack_with_inversion() {
        fn stub_builtin(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_matches!(
                    env.stack.as_slice(),
                    [Frame::Condition, Frame::Builtin { .. }]
                );
                Default::default()
            })
        }

        let mut env = Env::new_virtual();
        env.builtins
            .insert("foo", Builtin::new(Special, stub_builtin));
        let pipeline: syntax::Pipeline = "! foo".parse().unwrap();
        let result = pipeline.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
    }

    #[test]
    fn process_group_id_of_job_controlled_pipeline() {
        fn stub_builtin(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            let pgid = env.system.getpgrp().0 as _;
            Box::pin(async move { yash_env::builtin::Result::new(ExitStatus(pgid)) })
        }

        in_virtual_system(|mut env, state| async move {
            env.builtins
                .insert("foo", Builtin::new(Special, stub_builtin));
            env.options.set(Monitor, On);
            stub_tty(&state);

            // TODO Better test all pipeline component exit statuses
            let pipeline: syntax::Pipeline = "foo | foo".parse().unwrap();
            let result = pipeline.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_ne!(env.exit_status, ExitStatus(env.main_pgid.0 as _));

            // The shell should come back to the foreground after running the pipeline
            assert_eq!(state.borrow().foreground, Some(env.main_pgid));
        })
    }

    #[test]
    fn job_controlled_suspended_pipeline_in_job_list() {
        in_virtual_system(|mut env, state| async move {
            env.builtins.insert("return", return_builtin());
            env.builtins.insert("suspend", suspend_builtin());
            env.options.set(Monitor, On);
            stub_tty(&state);

            let pipeline: syntax::Pipeline = "return -n 0 | suspend x".parse().unwrap();
            let result = pipeline.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::from(SIGSTOP));

            assert_eq!(env.jobs.len(), 1);
            let job = env.jobs.iter().next().unwrap().1;
            assert!(job.job_controlled);
            assert_eq!(job.state, ProcessState::stopped(SIGSTOP));
            assert!(job.state_changed);
            assert_eq!(job.name, "return -n 0 | suspend x");
        })
    }

    #[test]
    fn pipe_set_shift_to_first_command() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut pipes = PipeSet::new();

        let result = pipes.shift(&mut env, true);
        assert_eq!(result, Ok(()));
        assert_eq!(pipes.read_previous, None);
        assert_eq!(pipes.next, Some((Fd(3), Fd(4))));
        let state = state.borrow();
        let process = &state.processes[&process_id];
        assert_eq!(process.fds().get(&Fd(3)).unwrap().flags, EnumSet::empty());
        assert_eq!(process.fds().get(&Fd(4)).unwrap().flags, EnumSet::empty());
    }

    #[test]
    fn pipe_set_shift_to_middle_command() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut pipes = PipeSet::new();

        let _ = pipes.shift(&mut env, true);
        let result = pipes.shift(&mut env, true);
        assert_eq!(result, Ok(()));
        assert_eq!(pipes.read_previous, Some(Fd(3)));
        assert_eq!(pipes.next, Some((Fd(4), Fd(5))));
        let state = state.borrow();
        let process = &state.processes[&process_id];
        assert_eq!(process.fds().get(&Fd(3)).unwrap().flags, EnumSet::empty());
        assert_eq!(process.fds().get(&Fd(4)).unwrap().flags, EnumSet::empty());
        assert_eq!(process.fds().get(&Fd(5)).unwrap().flags, EnumSet::empty());
    }

    #[test]
    fn pipe_set_shift_to_last_command() {
        let system = VirtualSystem::new();
        let process_id = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut pipes = PipeSet::new();

        let _ = pipes.shift(&mut env, true);
        let result = pipes.shift(&mut env, false);
        assert_eq!(result, Ok(()));
        assert_eq!(pipes.read_previous, Some(Fd(3)));
        assert_eq!(pipes.next, None);
        let state = state.borrow();
        let process = &state.processes[&process_id];
        assert_eq!(process.fds().get(&Fd(3)).unwrap().flags, EnumSet::empty());
    }

    // TODO test PipeSet::move_to_stdin_stdout
}
