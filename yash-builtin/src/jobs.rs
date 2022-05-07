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

//! Jobs built-in
//!
//! TODO Elaborate

use crate::common::arg::parse_arguments;
use crate::common::arg::Mode;
use crate::common::print_error_message;
use crate::common::Print;
use std::fmt::Write;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::job::WaitStatusEx;
use yash_env::semantics::Field;
use yash_env::Env;

/// Implementation of the jobs built-in.
pub async fn builtin_body(env: &mut Env, args: Vec<Field>) -> Result {
    let (_options, _operands) = match parse_arguments(&[], Mode::default(), args) {
        Ok(result) => result,
        Err(error) => return print_error_message(env, &error).await,
    };

    // Print jobs.
    let mut print = String::new();
    let current_job_index = env.jobs.current_job();
    let previous_job_index = env.jobs.previous_job();

    env.jobs.drain_filter(|index, mut job| {
        // Add a line of report
        use yash_env::job::fmt::{Marker, Report};
        let report = Report {
            index,
            marker: if current_job_index == Some(index) {
                Marker::CurrentJob
            } else if previous_job_index == Some(index) {
                Marker::PreviousJob
            } else {
                Marker::None
            },
            job: &job,
        };
        writeln!(print, "{}", report).unwrap();

        job.status_reported();

        // Remove terminated jobs
        job.status.is_finished()
    });

    (env.print(&print).await, Continue(()))
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(env: &mut Env, args: Vec<Field>) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::future::FutureExt;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::job::Job;
    use yash_env::job::Pid;
    use yash_env::job::WaitStatus;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::trap::Signal;

    #[test]
    fn no_operands_no_jobs() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(from_utf8(&stdout.content), Ok(""));
    }

    #[test]
    fn no_operands_some_jobs() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid::from_raw(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid::from_raw(72));
        job.status = WaitStatus::Stopped(Pid::from_raw(72), Signal::SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(
            from_utf8(&stdout.content),
            Ok("[1] - Running              echo first\n[2] + Stopped(SIGSTOP)     echo second\n")
        );
    }

    #[test]
    fn finished_jobs_are_removed_with_no_operands() {
        let mut env = Env::new_virtual();

        let mut job = Job::new(Pid::from_raw(11));
        job.name = "echo running".to_string();
        let i11 = env.jobs.add(job);

        let mut job = Job::new(Pid::from_raw(12));
        job.status = WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGTSTP);
        job.name = "echo stopped".to_string();
        let i12 = env.jobs.add(job);

        let mut job = Job::new(Pid::from_raw(13));
        job.status = WaitStatus::Continued(Pid::from_raw(13));
        job.name = "echo continued".to_string();
        let i13 = env.jobs.add(job);

        let mut job = Job::new(Pid::from_raw(14));
        job.status = WaitStatus::Exited(Pid::from_raw(14), 42);
        job.name = "echo exited".to_string();
        let i14 = env.jobs.add(job);

        let mut job = Job::new(Pid::from_raw(15));
        job.status = WaitStatus::Signaled(Pid::from_raw(15), Signal::SIGINT, false);
        job.name = "echo signaled".to_string();
        let i15 = env.jobs.add(job);

        let mut job = Job::new(Pid::from_raw(16));
        job.status = WaitStatus::Signaled(Pid::from_raw(16), Signal::SIGQUIT, true);
        job.name = "echo core dumped".to_string();
        let i16 = env.jobs.add(job);

        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        assert_matches!(env.jobs.get(i11), Some(_));
        assert_matches!(env.jobs.get(i12), Some(_));
        assert_matches!(env.jobs.get(i13), Some(_));
        assert_matches!(env.jobs.get(i14), None);
        assert_matches!(env.jobs.get(i15), None);
        assert_matches!(env.jobs.get(i16), None);
    }

    // TODO specifying job IDs
    // TODO finished jobs are removed with job ID
    // TODO specifying job IDs without the initial %
    // TODO specifying job IDs of non-existing jobs
    // TODO specifying ambiguous job ID
}
