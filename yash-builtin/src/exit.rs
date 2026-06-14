// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Exit built-in
//!
//! This module implements the [`exit` built-in], which causes the currently
//! executing shell to exit.
//!
//! [`exit` built-in]: https://magicant.github.io/yash-rs/builtins/exit.html
//!
//! # Implementation notes
//!
//! This implementation of the built-in does not actually exit the shell, but
//! returns a [`Result`] having a [`Divert::Exit`]. The caller is responsible
//! for handling the divert value and exiting the process.
//!
//! - If an operand specifies an exit status, the divert value will contain the
//!   specified exit status. The caller should use it as the exit status of the
//!   process.
//! - If no operand is given, the divert value will contain no exit status. The
//!   built-in's exit status is the current value of `$?`, and the caller should
//!   use it as the exit status of the process. However, if the built-in is
//!   invoked in a trap, the caller should use the value of `$?` before entering
//!   trap.
//!
//! The exit status is meant to be passed to
//! [`exit_or_raise`](yash_env::semantics::exit_or_raise) to exit (or terminate)
//! the shell process properly.
//!
//! In case of an error, the result will have a [`Divert::Interrupt`] value
//! instead, in which case the shell will not exit if it is interactive.

use crate::common::report::{report_error, syntax_error};
use crate::common::syntax::{Mode, OptionSpec, parse_arguments};
use std::num::ParseIntError;
use std::ops::ControlFlow::Break;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::input::SuspendedJobsGuardConfig;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::source::Location;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

const OPTIONS: &[OptionSpec] = &[OptionSpec::new().short('f').long("force")];

// TODO Split into syntax and semantics submodules

async fn operand_parse_error<S: Isatty + WriteAll>(
    env: &mut Env<S>,
    location: &Location,
    error: ParseIntError,
) -> Result {
    syntax_error(env, &error.to_string(), location).await
}

/// Entry point for executing the `exit` built-in
///
/// See the [module-level documentation](self) for details.
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> Result
where
    S: Isatty + WriteAll,
{
    let (options, args) = match parse_arguments(OPTIONS, Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };
    let force = options.iter().any(|o| o.spec.get_short() == Some('f'));

    if let Some(arg) = args.get(1) {
        return syntax_error(env, "too many operands", &arg.origin).await;
    }

    let exit_status = match args.first() {
        None => None,
        Some(arg) => match arg.value.parse() {
            Ok(exit_status) if exit_status >= 0 => Some(ExitStatus(exit_status)),
            Ok(_) => return syntax_error(env, "negative exit status", &arg.origin).await,
            Err(e) => return operand_parse_error(env, &arg.origin, e).await,
        },
    };
    // TODO: skip this check in PosixlyCorrect mode
    if !force
        && env.is_interactive()
        && let Some(config) = env.any.get::<SuspendedJobsGuardConfig>()
        && env.jobs.iter().any(|(_, job)| job.state.is_stopped())
    {
        env.system.print_error(&config.message).await;
        return Result::with_exit_status_and_divert(
            ExitStatus::ERROR,
            Break(Divert::Interrupt(None)),
        );
    }

    Result::with_exit_status_and_divert(env.exit_status, Break(Divert::Exit(exit_status)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::job::{Job, Pid, ProcessState};
    use yash_env::option::On;
    use yash_env::option::Option::Interactive;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::system::Concurrent;
    use yash_env::system::r#virtual::SIGTSTP;
    use yash_env::test_helper::assert_stderr;

    #[test]
    fn exit_without_arguments_with_exit_status_0() {
        let mut env = Env::new_virtual();
        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::SUCCESS, Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_without_arguments_with_non_zero_exit_status() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(42);
        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus(42), Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_with_exit_status_operand() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["123"]);
        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result = Result::with_exit_status_and_divert(
            ExitStatus::SUCCESS,
            Break(Divert::Exit(Some(ExitStatus(123)))),
        );
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_with_negative_exit_status_operand() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        }));
        let args = Field::dummies(["-1"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("-1"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn exit_with_non_integer_exit_status_operand() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        }));
        let args = Field::dummies(["foo"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("foo"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn exit_with_too_large_exit_status_operand() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        }));
        let args = Field::dummies(["999999999999999999999999999999"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("999999999999999999999999999999"),
                "stderr = {stderr:?}"
            )
        });
    }

    #[test]
    fn exit_with_too_many_arguments() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("exit"),
            is_special: true,
        }));
        let args = Field::dummies(["1", "2"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("too many operands"), "stderr = {stderr:?}")
        });
    }

    // TODO exit_with_invalid_option

    fn make_config() -> SuspendedJobsGuardConfig {
        SuspendedJobsGuardConfig::with_message(
            "# There are stopped jobs. Type `exit -f` to exit anyway.\n",
        )
    }

    fn interactive_env_with_suspended_job() -> (
        Env<Rc<Concurrent<VirtualSystem>>>,
        Rc<std::cell::RefCell<yash_env::system::r#virtual::SystemState>>,
    ) {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        let mut job = Job::new(Pid(42));
        job.state = ProcessState::stopped(SIGTSTP);
        env.jobs.insert(job);
        env.any.insert(Box::new(make_config()));
        (env, state)
    }

    #[test]
    fn exit_from_interactive_shell_without_suspended_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.any.insert(Box::new(make_config()));

        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::SUCCESS, Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
    }

    #[test]
    fn exit_from_interactive_shell_with_suspended_job_not_in_posix_mode() {
        let (mut env, state) = interactive_env_with_suspended_job();

        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("stopped jobs"), "stderr = {stderr:?}")
        });
    }

    // TODO exit_from_interactive_shell_with_suspended_job_in_posix_mode

    #[test]
    fn force_exit_from_interactive_shell_with_suspended_job() {
        let (mut env, state) = interactive_env_with_suspended_job();
        let args = Field::dummies(["-f"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::SUCCESS, Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn exit_from_interactive_shell_with_suspended_job_without_config() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        let mut job = Job::new(Pid(42));
        job.state = ProcessState::stopped(SIGTSTP);
        env.jobs.insert(job);
        // No SuspendedJobsGuardConfig in env.any — feature is disabled

        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::SUCCESS, Break(Divert::Exit(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }
}
