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

//! Simple command semantics for the absent target

use super::perform_assignments;
use crate::redir::RedirGuard;
use crate::xtrace::print;
use crate::xtrace::XTrace;
use crate::Handle;
use itertools::Itertools;
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
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

pub async fn execute_absent_target(
    env: &mut Env,
    assigns: &[Assign],
    redirs: &Rc<Vec<Redir>>,
    exit_status: ExitStatus,
) -> Result {
    // Perform redirections in a subshell
    let redir_exit_status = if let Some(redir) = redirs.first() {
        let first_redir_location = redir.body.operand().location.clone();
        let redirs_2 = Rc::clone(redirs);
        let subshell = Subshell::new(move |env, _job_control| {
            Box::pin(async move {
                let env = &mut RedirGuard::new(env);
                let mut xtrace = XTrace::from_options(&env.options);

                let redir_exit_status =
                    match env.perform_redirs(redirs_2.iter(), xtrace.as_mut()).await {
                        Ok(exit_status) => exit_status,
                        Err(e) => {
                            e.handle(env).await?;
                            return Break(Divert::Exit(None));
                        }
                    };

                print(env, xtrace).await;

                env.exit_status = redir_exit_status.unwrap_or(exit_status);
                Continue(())
            })
        })
        .job_control(JobControl::Foreground);

        match subshell.start_and_wait(env).await {
            Ok((pid, state)) => {
                if let ProcessState::Stopped(_) = state {
                    let mut job = Job::new(pid);
                    job.job_controlled = true;
                    job.state = state;
                    job.name = redirs
                        .iter()
                        .format_with(" ", |redir, f| f(&format_args!("{redir}")))
                        .to_string();
                    env.jobs.add(job);
                }

                state.try_into().unwrap()
            }
            Err(errno) => {
                print_error(
                    env,
                    "cannot start subshell to perform redirection".into(),
                    errno.desc().into(),
                    &first_redir_location,
                )
                .await;
                return Break(Divert::Interrupt(Some(ExitStatus::ERROR)));
            }
        }
    } else {
        exit_status
    };

    // Perform assignments in the current shell
    let mut xtrace = XTrace::from_options(&env.options);
    let assignment_exit_status = perform_assignments(env, assigns, false, xtrace.as_mut()).await?;
    print(env, xtrace).await;
    env.exit_status = assignment_exit_status.unwrap_or(redir_exit_status);
    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::tests::assert_stderr;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::option::State::On;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;
    use yash_syntax::syntax;

    #[test]
    fn simple_command_performs_redirection_with_absent_target() {
        in_virtual_system(|mut env, state| async move {
            let command: syntax::SimpleCommand = ">/tmp/foo".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            let file = state.borrow().file_system.get("/tmp/foo").unwrap();
            let file = file.borrow();
            assert_matches!(&file.body, FileBody::Regular { content, .. } => {
                assert_eq!(from_utf8(content), Ok(""));
            });
        });
    }

    #[test]
    fn simple_command_returns_command_substitution_exit_status_from_redirection() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let command: syntax::SimpleCommand = ">/tmp/foo$(return -n 42)".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.exit_status, ExitStatus(42));
        });
    }

    #[test]
    fn simple_command_handles_redirection_error_with_absent_target() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let command = &"$(return -n 11) < /no/such/file$(return -n 22)";
            let command: syntax::SimpleCommand = command.parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.exit_status, ExitStatus::ERROR);
        });
    }

    #[test]
    fn simple_command_handles_subshell_error_with_absent_target() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let command: syntax::SimpleCommand = ">/tmp/foo".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn simple_command_performs_assignment_with_absent_target() {
        let mut env = Env::new_virtual();
        let command: syntax::SimpleCommand = "a=b".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(
            env.variables.get("a").unwrap().value,
            Some(Value::scalar("b"))
        );
    }

    #[test]
    fn simple_command_returns_command_substitution_exit_status_from_assignment() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let command: syntax::SimpleCommand = "a=$(return -n 12)".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.exit_status, ExitStatus(12));
        })
    }

    #[test]
    fn simple_command_handles_assignment_error_with_absent_target() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut var = env.variables.get_or_new("a", Scope::Global);
        var.assign("", None).unwrap();
        var.make_read_only(Location::dummy("ROL"));
        let command: syntax::SimpleCommand = "a=b".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn xtrace_for_absent_target() {
        in_virtual_system(|mut env, state| async move {
            env.options.set(yash_env::option::XTrace, On);

            let command: syntax::SimpleCommand = "FOO=bar 3>/dev/null".parse().unwrap();
            let _ = command.execute(&mut env).await;

            assert_stderr(&state, |stderr| {
                assert_eq!(stderr, "3>/dev/null\nFOO=bar\n");
            });
        });
    }
}
