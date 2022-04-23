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

//! Wait built-in
//!
//! The wait built-in waits for asynchronous jobs to finish.
//!
//! # Syntax
//!
//! ```sh
//! wait [job_id_or_process_id...]
//! ```
//!
//! # Options
//!
//! None
//!
//! # Operands
//!
//! An operand can be a job ID or decimal process ID, specifying which job to
//! wait for.
//!
//! TODO Elaborate on syntax of job ID
//!
//! If you don't specify any operand, the built-in waits for all existing
//! asynchronous jobs.
//!
//! # Exit status
//!
//! If you specify one or more operands, the built-in returns the exit status of
//! the job specified by the last operand. If there is no operand, the exit
//! status is 0.
//!
//! If the built-in was interrupted by a signal, the exit status indicates the
//! signal.
//!
//! # Errors
//!
//! TBD
//!
//! # Portability
//!
//! The wait built-in is contained in the POSIX standard.
//!
//! The exact value of an exit status resulting from a signal is
//! implementation-dependent.

use crate::common::arg::parse_arguments;
use crate::common::arg::Mode;
use crate::common::print_error_message;
use std::fmt::Display;
use std::fmt::Formatter;
use std::future::Future;
use std::num::ParseIntError;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::job::Pid;
use yash_env::job::WaitStatus;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

// TODO Parse as a job ID if an operand starts with %
// TODO Treat an unknown job as terminated with exit status 127
// TODO Treat a suspended job as terminated if it is job-controlled.
// TODO Interruption by trap
// TODO Allow interrupting with SIGINT if interactive

enum JobSpecError {
    ParseInt(Field, ParseIntError),
    NonPositive(Field),
}

impl JobSpecError {
    fn field(&self) -> &Field {
        match self {
            JobSpecError::ParseInt(field, _) => field,
            JobSpecError::NonPositive(field) => field,
        }
    }
}

impl Display for JobSpecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JobSpecError::ParseInt(field, error) => write!(f, "{}: {}", field.value, error),
            JobSpecError::NonPositive(field) => {
                write!(f, "{}: non-positive process ID", field.value)
            }
        }
    }
}

impl MessageBase for JobSpecError {
    fn message_title(&self) -> std::borrow::Cow<str> {
        "invalid job specification".into()
    }
    fn main_annotation(&self) -> Annotation {
        Annotation::new(
            AnnotationType::Error,
            self.to_string().into(),
            &self.field().origin,
        )
    }
}

/// Implementation of the wait built-in.
pub async fn builtin_body(env: &mut Env, args: Vec<Field>) -> Result {
    let (_options, operands) = match parse_arguments(&[], Mode::default(), args) {
        Ok(result) => result,
        Err(error) => return print_error_message(env, &error).await,
    };

    if operands.is_empty() {
        while !env.jobs.is_empty() {
            match env.wait_for_subshell(Pid::from_raw(-1)).await {
                Err(Errno::ECHILD) => break,
                Err(Errno::EINTR) => todo!("signal interruption"),
                Err(_) => todo!("handle unexpected error"),
                Ok(WaitStatus::Exited(pid, _) | WaitStatus::Signaled(pid, _, _)) => {
                    if let Some(index) = env.jobs.job_index_by_pid(pid) {
                        env.jobs.remove_job(index);
                    }
                }
                Ok(_) => (),
            }
        }
        (ExitStatus::SUCCESS, Continue(()))
    } else {
        let mut exit_status = ExitStatus::SUCCESS;

        for job_spec in operands {
            let pid = match job_spec.value.parse() {
                Ok(pid) if pid > 0 => pid,
                Ok(_) => {
                    return print_error_message(env, &JobSpecError::NonPositive(job_spec)).await
                }
                Err(e) => {
                    return print_error_message(env, &JobSpecError::ParseInt(job_spec, e)).await
                }
            };

            match env.wait_for_subshell(Pid::from_raw(pid)).await {
                Err(Errno::ECHILD) => exit_status = ExitStatus::NOT_FOUND,
                Err(Errno::EINTR) => todo!("signal interruption"),
                Ok(WaitStatus::Exited(_pid, exit_status_value)) => {
                    exit_status = ExitStatus(exit_status_value);
                }
                Ok(WaitStatus::Signaled(_pid, _signal, _core_dumped)) => {
                    todo!("handle signaled job")
                }
                Ok(WaitStatus::Stopped(_pid, _signal)) => todo!("handle stopped job"),
                Ok(WaitStatus::Continued(_pid)) => todo!("handle continued job"),
                _ => todo!("handle unexpected error"),
            }
        }

        (exit_status, Continue(()))
    }
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::in_virtual_system;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::job::Job;
    use yash_env::stack::Frame;
    use yash_env::system::r#virtual::ProcessState;
    use yash_env::VirtualSystem;

    #[test]
    fn wait_no_operands_no_jobs() {
        in_virtual_system(|mut env, _pid, _state| async move {
            // Start a child process, but don't turn it into a job.
            env.start_subshell(|_| Box::pin(futures_util::future::pending()))
                .await
                .unwrap();

            let result = builtin_body(&mut env, vec![]).await;
            assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        })
    }

    #[test]
    fn wait_no_operands_some_jobs() {
        in_virtual_system(|mut env, pid, state| async move {
            for i in 1..=2 {
                let pid = env
                    .start_subshell(move |env| {
                        Box::pin(async move {
                            env.exit_status = ExitStatus(i);
                            Continue(())
                        })
                    })
                    .await
                    .unwrap();
                env.jobs.add_job(Job::new(pid));
            }

            let result = builtin_body(&mut env, vec![]).await;
            assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
            assert_eq!(env.jobs.job_count(), 0);

            let state = state.borrow();
            for (cpid, process) in &state.processes {
                if *cpid != pid {
                    assert!(!process.state_has_changed());
                    assert_matches!(process.state(), ProcessState::Exited(exit_status) => {
                        assert_ne!(exit_status, ExitStatus::SUCCESS);
                    });
                }
            }
        })
    }

    #[test]
    fn wait_no_operands_false_job() {
        let mut env = Env::new_virtual();

        // Add a job that is not a proper subshell.
        env.jobs.add_job(Job::new(Pid::from_raw(1)));

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
    }

    #[test]
    fn wait_some_operands() {
        in_virtual_system(|mut env, pid, state| async move {
            let mut pids = Vec::new();
            for i in 5..=6 {
                pids.push(
                    env.start_subshell(move |env| {
                        Box::pin(async move {
                            env.exit_status = ExitStatus(i);
                            Continue(())
                        })
                    })
                    .await
                    .unwrap(),
                );
            }

            let args = pids
                .iter()
                .map(|pid| Field::dummy(pid.to_string()))
                .collect();
            let result = builtin_body(&mut env, args).await;
            assert_eq!(result, (ExitStatus(6), Continue(())));

            let state = state.borrow();
            for (cpid, process) in &state.processes {
                if *cpid != pid {
                    assert!(!process.state_has_changed());
                    assert_matches!(process.state(), ProcessState::Exited(exit_status) => {
                        assert_ne!(exit_status, ExitStatus::SUCCESS);
                    });
                }
            }
        })
    }

    #[test]
    fn wait_unknown_process_id() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["9999999"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::NOT_FOUND, Continue(())));
    }

    #[test]
    fn non_numeric_operand() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("wait"),
        });
        let args = Field::dummies(["abc"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::ERROR, Continue(())));

        let state = state.borrow();
        let file = state.file_system.get("/dev/stderr").unwrap().borrow();
        assert_ne!(from_utf8(&file.content).unwrap(), "");
    }

    #[test]
    fn non_positive_process_id() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("wait"),
        });
        let args = Field::dummies(["0"]);

        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::ERROR, Continue(())));

        let state = state.borrow();
        let file = state.file_system.get("/dev/stderr").unwrap().borrow();
        assert_ne!(from_utf8(&file.content).unwrap(), "");
    }
}
