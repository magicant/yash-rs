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
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::job::Pid;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::Env;

// TODO Wait for jobs specified by operands
// TODO Parse as a job ID if an operand starts with %
// TODO Treat an unknown job as terminated with exit status 127
// TODO Treat a suspended job as terminated if it is job-controlled.
// TODO Interruption by trap
// TODO Allow interrupting with SIGINT if interactive

/// Implementation of the wait built-in.
pub async fn builtin_body(env: &mut Env, args: Vec<Field>) -> Result {
    let (_options, operands) = match parse_arguments(&[], Mode::default(), args) {
        Ok(result) => result,
        Err(error) => return print_error_message(env, &error).await,
    };

    if operands.is_empty() {
        loop {
            match env.wait_for_subshell(Pid::from_raw(-1)).await {
                Err(Errno::ECHILD) => break,
                Err(Errno::EINTR) => todo!("signal interruption"),
                Err(_) => todo!("handle unexpected error"),
                Ok(_) => (),
            }
        }
        (ExitStatus::SUCCESS, Continue(()))
    } else {
        todo!()
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
    use yash_env::system::r#virtual::ProcessState;

    #[test]
    fn wait_no_operands_no_jobs() {
        let mut env = Env::new_virtual();
        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
    }

    #[test]
    fn wait_no_operands_some_jobs() {
        in_virtual_system(|mut env, pid, state| async move {
            for i in 1..=2 {
                env.start_subshell(move |env| {
                    Box::pin(async move {
                        env.exit_status = ExitStatus(i);
                        Continue(())
                    })
                })
                .await
                .unwrap();
            }

            let result = builtin_body(&mut env, vec![]).await;
            assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

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
}
