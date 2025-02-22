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
//! The **`jobs`** built-in reports job status.
//!
//! # Synopsis
//!
//! ```sh
//! jobs [-lnprst] [job_id…]
//! ```
//!
//! # Description
//!
//! The jobs built-in prints information about jobs the shell is currently
//! controlling, one line for each job. The results follow the
//! [format](yash_env::job::fmt) specified by the POSIX.
//!
//! When the built-in reports a finished job (either exited or signaled), it
//! removes the job from the current execution environment.
//!
//! # Options
//!
//! ## Format
//!
//! By default, the results are printed in the standard format defined in the
//! [`yash_env::job::fmt`] module. You can use two options to change the output.
//!
//! The **`-l`** (**`--verbose`**) option uses the alternate format, which
//! inserts the process ID before each job state. The **`-p`**
//! (**`--pgid-only`**) option only prints the process ID of each job.
//!
//! ## Filtering
//!
//! TODO `-n`, `-r`, `-s`, `-t`
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
//! When the built-in is used in a subshell, the built-in reports not only jobs
//! that were started in the subshell but also jobs that were started in the
//! parent shell. This behavior is not portable and is subject to change.
//!
//! The POSIX standard only defines the `-l` and `-p` options. Other options are
//! non-portable extensions.
//!
//! According to POSIX, the `-p` option takes precedence over the `-l` option.
//! In many other shells, however, the last specified one is effective.
//!
//! A portable job ID must start with a `%`. If an operand does not have a
//! leading `%`, the built-in assumes one silently, which is not portable.

use crate::common::output;
use crate::common::report_error;
use crate::common::report_failure;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::job::fmt::Accumulator;
use yash_env::job::id::FindError;
use yash_env::job::id::parse;
use yash_env::job::id::parse_tail;
use yash_env::semantics::Field;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;

// TODO Split into syntax and semantics submodules

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::new().short('l').long("verbose"),
    OptionSpec::new().short('p').long("pgid-only"),
];

fn find_error_message(error: FindError, operand: &Field) -> Message {
    Message {
        r#type: AnnotationType::Error,
        title: "cannot report job status".into(),
        annotations: vec![Annotation::new(
            AnnotationType::Error,
            format!("{:?}: {}", &operand.value, error).into(),
            &operand.origin,
        )],
        footers: vec![],
    }
}

/// Entry point for executing the `jobs` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let (options, operands) = match parse_arguments(OPTIONS, Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };

    let mut accumulator = Accumulator {
        current_job_index: env.jobs.current_job(),
        previous_job_index: env.jobs.previous_job(),
        show_pid: false,
        pgid_only: false,
        print: String::new(),
        indices_reported: Vec::new(),
    };

    // Apply options
    for option in options {
        match option.spec.get_short() {
            Some('l') => accumulator.show_pid = true,
            Some('p') => accumulator.pgid_only = true,
            _ => unreachable!("unhandled option: {:?}", option),
        }
    }

    if operands.is_empty() {
        // Report all jobs
        for (index, job) in &env.jobs {
            accumulator.add(index, job, &env.system)
        }
    } else {
        // Report jobs specified by the operands
        for operand in operands {
            let job_id = parse(&operand.value).unwrap_or_else(|_| parse_tail(&operand.value));
            match job_id.find(&env.jobs) {
                Ok(index) => accumulator.add(index, &env.jobs[index], &env.system),
                Err(error) => {
                    return report_failure(env, find_error_message(error, &operand)).await;
                }
            }
        }
    }

    let result = output(env, &accumulator.print).await;

    if result.exit_status().is_successful() {
        for index in accumulator.indices_reported {
            let mut job = env.jobs.get_mut(index).unwrap();
            if job.state.is_alive() {
                job.state_reported();
            } else {
                env.jobs.remove(index);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::future::FutureExt;
    use std::rc::Rc;
    use yash_env::io::Fd;
    use yash_env::job::Job;
    use yash_env::job::Pid;
    use yash_env::job::ProcessResult;
    use yash_env::job::ProcessState;
    use yash_env::semantics::ExitStatus;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::system::r#virtual::{SIGINT, SIGQUIT, SIGSTOP, SIGTSTP};
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;

    #[test]
    fn no_operands_no_jobs() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn no_operands_some_jobs() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(
                stdout,
                "[1] - Running              echo first\n[2] + Stopped(SIGSTOP)     echo second\n"
            )
        });
    }

    #[test]
    fn finished_jobs_are_removed_with_no_operands() {
        let mut env = Env::new_virtual();

        let mut job = Job::new(Pid(11));
        job.name = "echo running".to_string();
        let i11 = env.jobs.add(job);

        let mut job = Job::new(Pid(12));
        job.state = ProcessState::stopped(SIGTSTP);
        job.name = "echo stopped".to_string();
        let i12 = env.jobs.add(job);

        let mut job = Job::new(Pid(13));
        job.state = ProcessState::Running;
        job.name = "echo continued".to_string();
        let i13 = env.jobs.add(job);

        let mut job = Job::new(Pid(14));
        job.state = ProcessState::exited(42);
        job.name = "echo exited".to_string();
        let i14 = env.jobs.add(job);

        let mut job = Job::new(Pid(15));
        job.state = ProcessState::Halted(ProcessResult::Signaled {
            signal: SIGINT,
            core_dump: false,
        });
        job.name = "echo signaled".to_string();
        let i15 = env.jobs.add(job);

        let mut job = Job::new(Pid(16));
        job.state = ProcessState::Halted(ProcessResult::Signaled {
            signal: SIGQUIT,
            core_dump: true,
        });
        job.name = "echo core dumped".to_string();
        let i16 = env.jobs.add(job);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

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
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);
        env.jobs.add(Job::new(Pid(100)));

        let args = Field::dummies(["%?first", "%2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(
                stdout,
                "[1] - Running              echo first\n[2] + Stopped(SIGSTOP)     echo second\n"
            )
        });
    }

    #[test]
    fn finished_jobs_are_removed_with_job_id() {
        let mut env = Env::new_virtual();

        // job that will not be removed because it's running
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        let i42 = env.jobs.add(job);

        // job that will be removed because it's finished
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::exited(0);
        job.name = "echo second".to_string();
        let i72 = env.jobs.add(job);

        // This one is also finished, but not removed because it's not reported.
        let mut job = Job::new(Pid(102));
        job.state = ProcessState::exited(0);
        job.name = "echo third".to_string();
        let i102 = env.jobs.add(job);

        let args = Field::dummies(["%?first", "%?second"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        assert!(env.jobs.get(i42).is_some());
        assert!(env.jobs.get(i72).is_none());
        assert!(env.jobs.get(i102).is_some());
    }

    #[test]
    fn specifying_job_ids_without_the_initial_percent() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(2));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);
        env.jobs.add(Job::new(Pid(100)));

        let args = Field::dummies(["?first", "2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(
                stdout,
                "[1] - Running              echo first\n[2] + Stopped(SIGSTOP)     echo second\n"
            )
        });
    }

    #[test]
    fn specifying_job_ids_of_non_existing_jobs() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.jobs.add(Job::new(Pid(2)));

        let args = Field::dummies(["%2"]);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("jobs"),
            is_special: false,
        }));
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::FAILURE));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("job not found"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn specifying_ambiguous_job_id() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);
        env.jobs.add(Job::new(Pid(100)));

        let args = Field::dummies(["%?first", "%echo"]);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("jobs"),
            is_special: false,
        }));
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::FAILURE));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("ambiguous"), "stderr = {stderr:?}")
        });
    }

    #[test]
    fn jobs_not_removed_in_case_of_error() {
        let mut system = Box::new(VirtualSystem::new());
        system.current_process_mut().close_fd(Fd::STDOUT);
        let mut env = Env::with_system(system);

        let mut job = Job::new(Pid(10));
        job.state = ProcessState::exited(0);
        job.name = "exit 0".to_string();
        let i10 = env.jobs.add(job);

        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("jobs"),
            is_special: false,
        }));
        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::FAILURE));
        assert_matches!(env.jobs.get(i10), Some(&Job { state, .. }) => {
            assert_eq!(state, ProcessState::exited(0));
        });
    }

    #[test]
    fn report_clears_state_changed_flag() {
        let mut env = Env::new_virtual();
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        let i42 = env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        let i72 = env.jobs.add(job);

        let args = Field::dummies(["%?sec"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        assert!(env.jobs[i42].state_changed);
        assert!(!env.jobs[i72].state_changed);
    }

    #[test]
    fn state_changed_flag_not_cleared_in_case_of_error() {
        let mut system = Box::new(VirtualSystem::new());
        system.current_process_mut().close_fd(Fd::STDOUT);
        let mut env = Env::with_system(system);
        let i72 = env.jobs.add(Job::new(Pid(72)));

        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("jobs"),
            is_special: false,
        }));
        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::FAILURE));
        assert!(env.jobs[i72].state_changed);
    }

    #[test]
    fn l_option() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let args = Field::dummies(["-l"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(
                stdout,
                "[1] -    42 Running              echo first
[2] +    72 Stopped(SIGSTOP)     echo second
"
            )
        });
    }

    #[test]
    #[ignore] // TODO Support parsing long option
    fn verbose_option() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let args = Field::dummies(["--verbose", "%?sec"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "[2] +    72 Stopped(SIGSTOP)     echo second\n")
        });
    }

    #[test]
    fn p_option() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let args = Field::dummies(["-p"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "42\n72\n"));
    }

    #[test]
    fn p_option_cancels_l_option() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let args = Field::dummies(["-pl"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "42\n72\n"));
    }

    #[test]
    #[ignore] // TODO Support parsing long option
    fn pgid_only_option() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut job = Job::new(Pid(42));
        job.name = "echo first".to_string();
        env.jobs.add(job);
        let mut job = Job::new(Pid(72));
        job.state = ProcessState::stopped(SIGSTOP);
        job.name = "echo second".to_string();
        env.jobs.add(job);

        let args = Field::dummies(["--pgid-only", "%?sec"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "72\n"));
    }
}
