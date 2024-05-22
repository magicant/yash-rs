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

//! Fg built-in
//!
//! The **`fg`** resumes a suspended job in the foreground.
//!
//! # Synopsis
//!
//! ```sh
//! fg [job_id]
//! ```
//!
//! # Description
//!
//! The built-in brings the specified job to the foreground and resumes its
//! execution by sending the `SIGCONT` signal to it. The built-in then waits for
//! the job to finish (or suspend again).
//!
//! If the resumed job finishes, it is removed from the [job list](JobList).
//! If the job gets suspended again, it is set as the [current
//! job](JobList::current_job).
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! Operand *job_id* specifies which job to resume. See the module documentation
//! of [`yash_env::job::id`] for the format of job IDs. If omitted, the built-in
//! resumes the [current job](JobList::current_job).
//!
//! (TODO: allow omitting the leading `%`)
//! (TODO: allow multiple operands)
//!
//! # Standard output
//!
//! The built-in writes the selected job name to the standard output.
//!
//! (TODO: print the job number as well)
//!
//! # Errors
//!
//! This built-in can be used only when the shell is in the foreground.
//! Otherwise, the shell will be suspended.
//!
//! The built-in fails if the specified job is not found, not job-controlled, or
//! not [owned] by the current shell environment.
//!
//! # Exit status
//!
//! The built-in returns the exit status of the resumed job. On error, it
//! returns a non-zero exit status.
//!
//! # Portability
//!
//! Many implementations allow omitting the leading `%` from job IDs and
//! specifying multiple job IDs at once, though this is not required by POSIX.
//!
//! # Implementation notes
//!
//! This implementation sends the `SIGCONT` signal even to jobs that are already
//! running. The signal is not sent to jobs that have already terminated, to
//! prevent unrelated processes that happen to have the same process IDs as the
//! jobs from receiving the signal.
//!
//! [owned]: yash_env::job::Job::is_owned

use crate::bg::OperandErrorKind;
use crate::bg::ResumeError;
use crate::common::report_error;
use crate::common::report_simple_failure;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use yash_env::io::Fd;
use yash_env::job::id::parse;
#[cfg(doc)]
use yash_env::job::JobList;
use yash_env::job::Pid;
use yash_env::job::ProcessState;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::Errno;
use yash_env::system::System as _;
use yash_env::system::SystemEx as _;
use yash_env::trap::Signal;
use yash_env::Env;

/// Waits for the specified job to finish (or suspend again).
async fn wait_until_halt(env: &mut Env, pid: Pid) -> Result<ProcessState, Errno> {
    loop {
        let (_pid, state) = env.wait_for_subshell(pid).await?;
        match state {
            ProcessState::Running => (),
            ProcessState::Halted(_) => return Ok(state),
        }
    }
}

/// Resumes the job at the specified index.
///
/// This function puts the target job in the foreground and sends the `SIGCONT`
/// signal to it. It then waits for the job to finish (or suspend again).
///
/// This function panics if there is no job at the specified index.
async fn resume_job_by_index(env: &mut Env, index: usize) -> Result<ProcessState, ResumeError> {
    let tty = env.get_tty()?;

    let job = &env.jobs[index];
    if !job.is_owned {
        return Err(ResumeError::Unowned);
    }
    if !job.job_controlled {
        return Err(ResumeError::Unmonitored);
    }

    let line = format!("{}\n", job.name);
    env.system.write_all(Fd::STDOUT, line.as_bytes()).await?;
    drop(line);

    let mut state = job.state;
    if state.is_alive() {
        // TODO Should we save/restore the terminal state?

        // Make sure to put the target job in the foreground before sending the
        // SIGCONT signal, or the job may be immediately re-suspended.
        env.system.tcsetpgrp_without_block(tty, job.pid)?;

        let pgid = -job.pid;
        env.system.kill(pgid, Signal::SIGCONT.into()).await?;

        // Wait for the job to finish (or suspend again).
        state = wait_until_halt(env, job.pid).await?;

        // Move the shell back to the foreground.
        env.system.tcsetpgrp_with_block(tty, env.main_pgid)?;
    }

    // Remove the job if it has finished.
    if !state.is_alive() {
        env.jobs.remove(index);
    }

    Ok(state)
}

/// Resumes the job specified by the operand.
async fn resume_job_by_id(env: &mut Env, job_id: &str) -> Result<ProcessState, OperandErrorKind> {
    let job_id = parse(job_id)?;
    let index = job_id.find(&env.jobs)?;
    Ok(resume_job_by_index(env, index).await?)
}

/// Entry point of the `fg` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    let (options, operands) = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };
    debug_assert_eq!(options, []);

    let result = if operands.is_empty() {
        if let Some(index) = env.jobs.current_job() {
            resume_job_by_index(env, index).await.map_err(Into::into)
        } else {
            return report_simple_failure(env, "there is no job").await;
        }
    } else if operands.len() > 1 {
        // TODO Support multiple operands
        return report_simple_failure(env, "too many operands").await;
    } else {
        resume_job_by_id(env, &operands[0].value).await
    };

    match result {
        Ok(state) => ExitStatus::try_from(state).unwrap().into(),
        Err(error) => report_simple_failure(env, &error.to_string()).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::in_virtual_system;
    use crate::tests::stub_tty;
    use futures_util::FutureExt as _;
    use std::cell::Cell;
    use std::rc::Rc;
    use yash_env::job::Job;
    use yash_env::option::Option::Monitor;
    use yash_env::option::State::On;
    use yash_env::subshell::JobControl;
    use yash_env::subshell::Subshell;
    use yash_env::system::r#virtual::Process;
    use yash_env::VirtualSystem;

    async fn suspend(env: &mut Env) {
        env.system
            .kill(env.system.getpid(), Some(Signal::SIGSTOP))
            .await
            .unwrap();
    }

    #[test]
    fn resume_job_by_index_resumes_job_in_foreground() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            let reached = Rc::new(Cell::new(false));
            let reached2 = Rc::clone(&reached);
            let subshell = Subshell::new(|env, _| {
                Box::pin(async move {
                    suspend(env).await;

                    // When resumed, the subshell should be in the foreground.
                    let tty = env.get_tty().unwrap();
                    assert_eq!(env.system.tcgetpgrp(tty).unwrap(), env.system.getpid());
                    reached2.set(true);
                })
            })
            .job_control(JobControl::Foreground);
            let (pid, subshell_state) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid);
            job.job_controlled = true;
            job.state = subshell_state;
            let index = env.jobs.add(job);

            resume_job_by_index(&mut env, index).await.unwrap();

            assert!(reached.get());
        })
    }

    #[test]
    fn resume_job_by_index_prints_job_name() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            let subshell =
                Subshell::new(|env, _| Box::pin(suspend(env))).job_control(JobControl::Foreground);
            let (pid, subshell_state) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid);
            job.job_controlled = true;
            job.state = subshell_state;
            "my job name".clone_into(&mut job.name);
            let index = env.jobs.add(job);

            resume_job_by_index(&mut env, index).await.unwrap();

            assert_stdout(&state, |stdout| assert_eq!(stdout, "my job name\n"));
        })
    }

    #[test]
    fn resume_job_by_index_returns_after_job_exits() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            let subshell = Subshell::new(|env, _| {
                Box::pin(async move {
                    suspend(env).await;
                    env.exit_status = ExitStatus(42);
                })
            })
            .job_control(JobControl::Foreground);
            let (pid, subshell_state) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid);
            job.job_controlled = true;
            job.state = subshell_state;
            let index = env.jobs.add(job);

            let result = resume_job_by_index(&mut env, index).await.unwrap();

            assert_eq!(result, ProcessState::exited(42));
            let state = state.borrow().processes[&pid].state();
            assert_eq!(state, ProcessState::exited(42));
            // The finished job should be removed from the job list.
            assert_eq!(env.jobs.get(index), None);
        })
    }

    #[test]
    fn resume_job_by_index_returns_after_job_suspends() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            let subshell = Subshell::new(|env, _| {
                Box::pin(async move {
                    suspend(env).await;
                    suspend(env).await;
                    unreachable!("child process should not be resumed twice");
                })
            })
            .job_control(JobControl::Foreground);
            let (pid, subshell_state) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid);
            job.job_controlled = true;
            job.state = subshell_state;
            let index = env.jobs.add(job);

            let result = resume_job_by_index(&mut env, index).await.unwrap();

            assert_eq!(result, ProcessState::stopped(Signal::SIGSTOP));
            let job_state = env.jobs[index].state;
            assert_eq!(job_state, ProcessState::stopped(Signal::SIGSTOP));
            let state = state.borrow().processes[&pid].state();
            assert_eq!(state, ProcessState::stopped(Signal::SIGSTOP));
        })
    }

    #[test]
    fn resume_job_by_index_moves_shell_back_to_foreground() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            let subshell =
                Subshell::new(|env, _| Box::pin(suspend(env))).job_control(JobControl::Foreground);
            let (pid, subshell_state) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid);
            job.job_controlled = true;
            job.state = subshell_state;
            let index = env.jobs.add(job);

            _ = resume_job_by_index(&mut env, index).await.unwrap();

            let foreground = state.borrow().foreground;
            assert_eq!(foreground, Some(env.main_pgid));
        })
    }

    #[test]
    fn resume_job_by_index_sends_no_sigcont_to_dead_process() {
        let system = VirtualSystem::new();
        stub_tty(&system.state);
        let mut env = Env::with_system(Box::new(system.clone()));
        let pid = Pid(123);
        let mut job = Job::new(pid);
        job.job_controlled = true;
        job.state = ProcessState::exited(12);
        let index = env.jobs.add(job);
        // This process (irrelevant to the job) happens to have the same PID as the job.
        let mut process = Process::with_parent_and_group(system.process_id, pid);
        _ = process.set_state(ProcessState::stopped(Signal::SIGSTOP));
        {
            let mut state = system.state.borrow_mut();
            state.processes.insert(pid, process);
        }

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();

        assert_eq!(result, Ok(ProcessState::exited(12)));
        // The finished job should be removed from the job list.
        assert_eq!(env.jobs.get(index), None);

        let state = system.state.borrow();
        // The process should not be resumed.
        assert_eq!(
            state.processes[&pid].state(),
            ProcessState::stopped(Signal::SIGSTOP),
        );
    }

    #[test]
    fn resume_job_by_index_rejects_unowned_job() {
        let system = VirtualSystem::new();
        stub_tty(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.is_owned = false;
        let index = env.jobs.add(job);

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();
        assert_eq!(result, Err(ResumeError::Unowned));
    }

    #[test]
    fn resume_job_by_index_rejects_unmonitored_job() {
        let system = VirtualSystem::new();
        stub_tty(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let index = env.jobs.add(Job::new(Pid(123)));

        let result = resume_job_by_index(&mut env, index).now_or_never().unwrap();
        assert_eq!(result, Err(ResumeError::Unmonitored));
    }

    #[test]
    fn main_without_operands_resumes_current_job() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            // previous job
            let subshell = Subshell::new(|env, _| {
                Box::pin(async move {
                    suspend(env).await;
                    unreachable!("previous job should not be resumed");
                })
            })
            .job_control(JobControl::Foreground);
            let (pid1, subshell_state1) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state1, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid1);
            job.job_controlled = true;
            job.state = subshell_state1;
            env.jobs.add(job);
            // current job
            let subshell =
                Subshell::new(|env, _| Box::pin(suspend(env))).job_control(JobControl::Foreground);
            let (pid2, subshell_state2) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state2, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid2);
            job.job_controlled = true;
            job.state = subshell_state2;
            let index2 = env.jobs.add(job);
            env.jobs.set_current_job(index2).unwrap();

            let result = main(&mut env, vec![]).await;

            assert_eq!(result, crate::Result::default());
            // The finished job should be removed from the job list.
            assert_eq!(env.jobs.get(index2), None);
            // The previous job should still be there.
            let state = state.borrow().processes[&pid1].state();
            assert_eq!(state, ProcessState::stopped(Signal::SIGSTOP));
        })
    }

    #[test]
    fn main_without_operands_fails_if_there_is_no_current_job() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));

        let result = main(&mut env, vec![]).now_or_never().unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("there is no job"), "{stderr:?}");
        });
    }

    #[test]
    fn main_with_operand_resumes_specified_job() {
        in_virtual_system(|mut env, state| async move {
            stub_tty(&state);
            env.options.set(Monitor, On);
            // previous job
            let subshell =
                Subshell::new(|env, _| Box::pin(suspend(env))).job_control(JobControl::Foreground);
            let (pid1, subshell_state1) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state1, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid1);
            job.job_controlled = true;
            job.state = subshell_state1;
            job.name = "previous job".to_string();
            let index1 = env.jobs.add(job);
            // current job
            let subshell = Subshell::new(|env, _| {
                Box::pin(async move {
                    suspend(env).await;
                    unreachable!("current job should not be resumed");
                })
            })
            .job_control(JobControl::Foreground);
            let (pid2, subshell_state2) = subshell.start_and_wait(&mut env).await.unwrap();
            assert_eq!(subshell_state2, ProcessState::stopped(Signal::SIGSTOP));
            let mut job = Job::new(pid2);
            job.job_controlled = true;
            job.state = subshell_state2;
            let index2 = env.jobs.add(job);
            env.jobs.set_current_job(index2).unwrap();

            let result = main(&mut env, Field::dummies(["%prev"])).await;

            assert_eq!(result, crate::Result::default());
            // The finished job should be removed from the job list.
            assert_eq!(env.jobs.get(index1), None);
            // The previous job should still be there.
            let state = state.borrow().processes[&pid2].state();
            assert_eq!(state, ProcessState::stopped(Signal::SIGSTOP));
        })
    }

    #[test]
    fn main_with_operand_fails_if_jobs_is_not_found() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system.clone()));
        let mut job = Job::new(Pid(123));
        job.job_controlled = true;
        job.name = "foo".to_string();
        env.jobs.add(job);

        let result = main(&mut env, Field::dummies(["%bar"]))
            .now_or_never()
            .unwrap();

        assert_eq!(result, crate::Result::from(ExitStatus::FAILURE));
        assert_stderr(&system.state, |stderr| {
            assert!(stderr.contains("not found"), "{stderr:?}");
        });
    }
}
