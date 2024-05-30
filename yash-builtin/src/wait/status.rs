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

//! Logic that decides when to stop waiting
//!
//! The [`wait_while_running`] function waits for a job to finish. It takes a
//! closure that tests if the job has finished. When the closure returns
//! [`ControlFlow::Break`], `wait_while_running` stops waiting and returns the
//! exit status.
//!
//! You can pass a closure that is created by [`job_status`] or
//! [`any_job_is_running`] to `wait_while_running`. The former tests if a
//! specific job has finished, and the latter tests if all jobs have finished.

use super::core::wait_for_any_job_or_trap;
use super::core::Error;
use std::ops::ControlFlow;
use yash_env::job::JobList;
use yash_env::job::ProcessResult;
use yash_env::job::ProcessState;
use yash_env::option::State;
use yash_env::semantics::ExitStatus;
use yash_env::Env;

/// Waits while the given job is running.
///
/// This function keeps calling [`wait_for_any_job_or_trap`] until the given
/// closure returns [`ControlFlow::Break`], whose value is returned from this
/// function.
pub async fn wait_while_running(
    env: &mut Env,
    job_status: &mut dyn FnMut(&mut JobList) -> ControlFlow<ExitStatus>,
) -> Result<ExitStatus, Error> {
    loop {
        if let ControlFlow::Break(exit_status) = job_status(&mut env.jobs) {
            return Ok(exit_status);
        }
        wait_for_any_job_or_trap(env).await?;
    }
}

/// Returns a closure that tests if the job with the given index has finished.
///
/// If the job at the given index is not found or is disowned, the closure
/// returns [`ControlFlow::Break`] having [`ExitStatus::NOT_FOUND`].
/// The disowned job is removed from the job list.
///
/// If the job has finished (either exited or signaled), the closure removes the
/// job from the job list and returns [`ControlFlow::Break`] with the job's exit
/// status. If `job_control` is `On` and the job has been stopped, the closure
/// returns [`ControlFlow::Break`] with an exit status that indicates the signal
/// that stopped the job.
/// Otherwise, the closure returns [`ControlFlow::Continue`].
pub fn job_status(
    index: usize,
    job_control: State,
) -> impl FnMut(&mut JobList) -> ControlFlow<ExitStatus> {
    move |jobs| {
        let Some(job) = jobs.get(index) else {
            return ControlFlow::Break(ExitStatus::NOT_FOUND);
        };

        if !job.is_owned {
            jobs.remove(index);
            return ControlFlow::Break(ExitStatus::NOT_FOUND);
        }

        if let ProcessState::Halted(result) = job.state {
            match result {
                ProcessResult::Exited(_) | ProcessResult::Signaled { .. } => {
                    jobs.remove(index);
                    return ControlFlow::Break(result.into());
                }
                ProcessResult::Stopped(_) => {
                    if job_control.into() {
                        return ControlFlow::Break(result.into());
                    }
                }
            }
        }

        ControlFlow::Continue(())
    }
}

/// Returns a closure that tests if any job is running.
///
/// The closure applies [`job_status`] to each job in the job list. If all jobs
/// have finished, the closure returns [`ControlFlow::Break`] with the exit
/// status of 0. Otherwise, the closure returns [`ControlFlow::Continue`].
pub fn any_job_is_running(
    job_control: State,
) -> impl FnMut(&mut JobList) -> ControlFlow<ExitStatus> {
    move |jobs| {
        let Some((max_index, _)) = jobs.iter().next_back() else {
            return ControlFlow::Break(ExitStatus::SUCCESS);
        };

        if (0..=max_index).all(|index| job_status(index, job_control)(jobs).is_break()) {
            ControlFlow::Break(ExitStatus::SUCCESS)
        } else {
            ControlFlow::Continue(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::job::{Job, Pid};
    use yash_env::option::State::{Off, On};
    use yash_env::system::r#virtual::{SIGABRT, SIGHUP, SIGSTOP, SIGTSTP};

    #[test]
    fn status_of_unknown_job() {
        let mut jobs = JobList::new();
        assert_eq!(
            job_status(0, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::NOT_FOUND),
        );
        assert_eq!(
            job_status(1, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::NOT_FOUND),
        );
        assert_eq!(
            job_status(0, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::NOT_FOUND),
        );
        assert_eq!(jobs.len(), 0);
    }

    #[test]
    fn status_of_exited_job() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.state = ProcessState::exited(0);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );
        assert_eq!(jobs.get(index), None);

        let mut job = Job::new(Pid(456));
        job.state = ProcessState::exited(42);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus(42)),
        );
        assert_eq!(jobs.get(index), None);
    }

    #[test]
    fn status_of_signaled_job() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.state = ProcessState::Halted(ProcessResult::Signaled {
            signal: SIGHUP,
            core_dump: false,
        });
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(SIGHUP)),
        );
        assert_eq!(jobs.get(index), None);

        let mut job = Job::new(Pid(456));
        job.state = ProcessState::Halted(ProcessResult::Signaled {
            signal: SIGABRT,
            core_dump: true,
        });
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(SIGABRT)),
        );
        assert_eq!(jobs.get(index), None);
    }

    #[test]
    fn status_of_stopped_job_without_job_control() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.state = ProcessState::stopped(SIGTSTP);
        let index = jobs.add(job);

        assert_eq!(job_status(index, Off)(&mut jobs), ControlFlow::Continue(()),);
        assert_eq!(jobs[index].pid, Pid(123));

        let mut job = Job::new(Pid(456));
        job.state = ProcessState::stopped(SIGSTOP);
        let index = jobs.add(job);

        assert_eq!(job_status(index, Off)(&mut jobs), ControlFlow::Continue(()),);
        assert_eq!(jobs[index].pid, Pid(456));
    }

    #[test]
    fn status_of_stopped_job_with_job_control() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.state = ProcessState::stopped(SIGTSTP);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(SIGTSTP)),
        );
        assert_eq!(jobs[index].pid, Pid(123));

        let mut job = Job::new(Pid(456));
        job.state = ProcessState::stopped(SIGSTOP);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(SIGSTOP)),
        );
        assert_eq!(jobs[index].pid, Pid(456));
    }

    #[test]
    fn status_of_continued_job() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.state = ProcessState::Running;
        let index = jobs.add(job);

        assert_eq!(job_status(index, Off)(&mut jobs), ControlFlow::Continue(()));
        assert_eq!(jobs[index].pid, Pid(123));

        let mut job = Job::new(Pid(456));
        job.state = ProcessState::Running;
        let index = jobs.add(job);

        assert_eq!(job_status(index, On)(&mut jobs), ControlFlow::Continue(()));
        assert_eq!(jobs[index].pid, Pid(456));
    }

    #[test]
    fn status_of_disowned_job() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.is_owned = false;
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::NOT_FOUND),
        );
        assert_eq!(jobs.get(index), None);
    }

    #[test]
    fn any_job_is_running_with_no_job() {
        let mut jobs = JobList::new();
        assert_eq!(
            any_job_is_running(Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );
        assert_eq!(
            any_job_is_running(On)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );
    }

    #[test]
    fn any_job_is_running_with_exited_jobs() {
        let mut jobs = JobList::new();
        let mut job = Job::new(Pid(123));
        job.state = ProcessState::exited(0);
        jobs.add(job);

        assert_eq!(
            any_job_is_running(Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );

        let mut job = Job::new(Pid(456));
        job.state = ProcessState::exited(42);
        jobs.add(job);

        assert_eq!(
            any_job_is_running(On)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );
    }

    #[test]
    fn any_job_is_running_with_running_job() {
        let mut jobs = JobList::new();
        jobs.add(Job::new(Pid(123)));

        assert_eq!(
            any_job_is_running(Off)(&mut jobs),
            ControlFlow::Continue(()),
        );

        jobs.add(Job::new(Pid(456)));

        assert_eq!(any_job_is_running(On)(&mut jobs), ControlFlow::Continue(()));

        // Exited jobs are ignored
        let mut job = Job::new(Pid(789));
        job.state = ProcessState::exited(0);
        jobs.add(job);

        assert_eq!(any_job_is_running(On)(&mut jobs), ControlFlow::Continue(()));
    }
}
