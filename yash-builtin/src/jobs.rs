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
//! The `jobs` built-in reports job status.
//!
//! # Syntax
//!
//! ```sh
//! jobs [-lnprst] [job_id…]
//! ```
//!
//! # Semantics
//!
//! The "jobs" built-in prints information about jobs the shell is currently
//! controlling, one line for each job. The results follow the
//! [format](yash_env::job::fmt) specified by the POSIX.
//!
//! When the built-in reports a finished job (either exited or signaled), it
//! removes the job from the current execution environment.
//!
//! # Options
//!
//! TODO `-l`, `-n`, `-p`, `-r`, `-s`, `-t`
//!
//! # Operands
//!
//! Each operand is parsed as a [job ID](yash_env::job::id) that specifies which
//! job to report. If no operands are given, the built-in prints all jobs.
//!
//! # Exit status
//!
//! `ExitStatus::SUCCESS` or `ExitStatus::FAILURE` depending on the results
//!
//! # Portability
//!
//! The current implementation of this built-in removes finished jobs from the
//! environment after reporting all jobs. This behavior should not be relied
//! upon. The following script shows a "job not found" error in many other
//! shells because the built-in removes the job when processing the first
//! operand so the job is gone when the second is processed:
//!
//! ```sh
//! sleep 0&
//! jobs %sleep %sleep
//! ```
//!
//! The POSIX standard defines the `-l` and `-p` option. Other options are
//! non-portable extension.
//!
//! A portable job ID must start with a `%`. If an operand does not have a
//! leading `%`, the built-in assumes one silently, which is not portable.

use crate::common::arg::parse_arguments;
use crate::common::arg::Mode;
use crate::common::arg::OptionSpec;
use crate::common::print_error_message;
use crate::common::Print;
use std::fmt::Write;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::job::id::parse;
use yash_env::job::id::parse_tail;
use yash_env::job::id::FindError;
use yash_env::job::JobRefMut;
use yash_env::job::WaitStatusEx;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;

const OPTIONS: &[OptionSpec] = &[OptionSpec::new().short('l').long("verbose")];

enum Format {
    Standard,
    Verbose,
    // TODO PgidOnly,
}

struct Accumulator {
    current_job_index: Option<usize>,
    previous_job_index: Option<usize>,
    format: Format,
    print: String,
    indices_to_remove: Vec<usize>,
}

impl Accumulator {
    /// Processes one job.
    ///
    /// 1. Formats a job report in `self.print` so it can be printed later.
    /// 1. Clears the `status_changed` flag of the job.
    /// 1. Remembers the job index in `self.indices_to_remove` so the job can be
    ///    removed later.
    fn report(&mut self, index: usize, mut job: JobRefMut) {
        use yash_env::job::fmt::{Marker, Report};
        let report = Report {
            index,
            marker: if self.current_job_index == Some(index) {
                Marker::CurrentJob
            } else if self.previous_job_index == Some(index) {
                Marker::PreviousJob
            } else {
                Marker::None
            },
            job: &job,
        };
        match self.format {
            Format::Standard => writeln!(self.print, "{}", report).unwrap(),
            Format::Verbose => writeln!(self.print, "{:#}", report).unwrap(),
        }

        job.status_reported();

        if job.status.is_finished() {
            self.indices_to_remove.push(index);
        }
    }
}

fn find_error_message(error: FindError, operand: &Field) -> Message {
    Message {
        r#type: AnnotationType::Error,
        title: "cannot report job status".into(),
        annotations: vec![Annotation::new(
            AnnotationType::Error,
            format!("{:?}: {}", &operand.value, error).into(),
            &operand.origin,
        )],
    }
}

/// Implementation of the jobs built-in.
pub async fn builtin_body(env: &mut Env, args: Vec<Field>) -> Result {
    let (options, operands) = match parse_arguments(OPTIONS, Mode::default(), args) {
        Ok(result) => result,
        Err(error) => return print_error_message(env, &error).await,
    };

    let mut accumulator = Accumulator {
        current_job_index: env.jobs.current_job(),
        previous_job_index: env.jobs.previous_job(),
        format: Format::Standard,
        print: String::new(),
        indices_to_remove: Vec::new(),
    };

    // Parse options
    for option in options {
        match option.spec.get_short() {
            Some('l') => accumulator.format = Format::Verbose,
            _ => unreachable!("unhandled option: {:?}", option),
        }
    }

    if operands.is_empty() {
        // Report all jobs
        for (index, job) in &mut env.jobs {
            accumulator.report(index, job)
        }
    } else {
        // Report jobs specified by the operands
        for operand in operands {
            let job_id = parse(&operand.value).unwrap_or_else(|_| parse_tail(&operand.value));
            match job_id.find(&env.jobs) {
                Ok(index) => {
                    let job = env.jobs.get_mut(index).unwrap();
                    accumulator.report(index, job)
                }
                Err(error) => {
                    print_error_message(env, find_error_message(error, &operand)).await;
                    return (ExitStatus::FAILURE, Continue(()));
                }
            }
        }
    }

    let exit_status = env.print(&accumulator.print).await;

    if exit_status == ExitStatus::SUCCESS {
        for index in accumulator.indices_to_remove {
            env.jobs.remove(index);
        }
    }

    (exit_status, Continue(()))
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
    use yash_env::io::Fd;
    use yash_env::job::Job;
    use yash_env::job::Pid;
    use yash_env::job::WaitStatus;
    use yash_env::stack::Frame;
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

    #[test]
    fn specifying_valid_job_ids() {
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
        env.jobs.add(Job::new(Pid::from_raw(100)));

        let args = Field::dummies(["%?first", "%2"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(
            from_utf8(&stdout.content),
            Ok("[1] - Running              echo first\n[2] + Stopped(SIGSTOP)     echo second\n")
        );
    }

    #[test]
    fn finished_jobs_are_removed_with_job_id() {
        let mut env = Env::new_virtual();

        // job that will not be removed because it's running
        let mut job = Job::new(Pid::from_raw(42));
        job.name = "echo first".to_string();
        let i42 = env.jobs.add(job);

        // job that will be removed because it's finished
        let mut job = Job::new(Pid::from_raw(72));
        job.status = WaitStatus::Exited(Pid::from_raw(72), 0);
        job.name = "echo second".to_string();
        let i72 = env.jobs.add(job);

        // This one is also finished, but not removed because it's not reported.
        let mut job = Job::new(Pid::from_raw(102));
        job.status = WaitStatus::Exited(Pid::from_raw(102), 0);
        job.name = "echo third".to_string();
        let i102 = env.jobs.add(job);

        let args = Field::dummies(["%?first", "%?second"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        assert!(env.jobs.get(i42).is_some());
        assert!(env.jobs.get(i72).is_none());
        assert!(env.jobs.get(i102).is_some());
    }

    #[test]
    fn specifying_job_ids_without_the_initial_percent() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid::from_raw(2));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid::from_raw(72));
        job.status = WaitStatus::Stopped(Pid::from_raw(72), Signal::SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);
        env.jobs.add(Job::new(Pid::from_raw(100)));

        let args = Field::dummies(["?first", "2"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(
            from_utf8(&stdout.content),
            Ok("[1] - Running              echo first\n[2] + Stopped(SIGSTOP)     echo second\n")
        );
    }

    #[test]
    fn specifying_job_ids_of_non_existing_jobs() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.jobs.add(Job::new(Pid::from_raw(2)));

        let args = Field::dummies(["%2"]);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("jobs"),
        });
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::FAILURE, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(from_utf8(&stdout.content), Ok(""));
        let stderr = state.file_system.get("/dev/stderr").unwrap().borrow();
        let stderr = from_utf8(&stderr.content).unwrap();
        assert!(stderr.contains("job not found"), "{:?}", stderr);
    }

    #[test]
    fn specifying_ambiguous_job_id() {
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
        env.jobs.add(Job::new(Pid::from_raw(100)));

        let args = Field::dummies(["%?first", "%echo"]);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("jobs"),
        });
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::FAILURE, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(from_utf8(&stdout.content), Ok(""));
        let stderr = state.file_system.get("/dev/stderr").unwrap().borrow();
        let stderr = from_utf8(&stderr.content).unwrap();
        assert!(stderr.contains("ambiguous"), "{:?}", stderr);
    }

    #[test]
    fn jobs_not_removed_in_case_of_error() {
        let mut system = Box::new(VirtualSystem::new());
        system.current_process_mut().close_fd(Fd::STDOUT);
        let mut env = Env::with_system(system);

        let mut job = Job::new(Pid::from_raw(10));
        job.status = WaitStatus::Exited(Pid::from_raw(10), 0);
        job.name = "exit 0".to_string();
        let i10 = env.jobs.add(job);

        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("jobs"),
        });
        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::FAILURE, Continue(())));
        assert_matches!(env.jobs.get(i10), Some(&Job { status, .. }) => {
            assert_eq!(status, WaitStatus::Exited(Pid::from_raw(10), 0));
        });
    }

    #[test]
    fn l_option() {
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

        let args = Field::dummies(["-l"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(
            from_utf8(&stdout.content),
            Ok("[1] -    42 Running              echo first
[2] +    72 Stopped(SIGSTOP)     echo second
")
        );
    }

    #[test]
    #[ignore] // TODO Support parsing long option
    fn verbose_option() {
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

        let args = Field::dummies(["--verbose", "%?sec"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(
            from_utf8(&stdout.content),
            Ok("[2] +    72 Stopped(SIGSTOP)     echo second\n")
        );
    }
}
