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
use yash_env::job::JobSet;
use yash_env::job::WaitStatus;
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
    job_status: &mut dyn FnMut(&mut JobSet) -> ControlFlow<ExitStatus>,
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
/// The disowned job is removed from the job set.
///
/// If the job has finished (either exited or signaled), the closure removes the
/// job from the job set and returns [`ControlFlow::Break`] with the job's exit
/// status. If `job_control` is `On` and the job has been stopped, the closure
/// returns [`ControlFlow::Break`] with an exit status that indicates the signal
/// that stopped the job.
/// Otherwise, the closure returns [`ControlFlow::Continue`].
pub fn job_status(
    index: usize,
    job_control: State,
) -> impl FnMut(&mut JobSet) -> ControlFlow<ExitStatus> {
    move |jobs| {
        let Some(job) = jobs.get(index) else {
            return ControlFlow::Break(ExitStatus::NOT_FOUND);
        };

        if !job.is_owned {
            jobs.remove(index);
            return ControlFlow::Break(ExitStatus::NOT_FOUND);
        }

        match job.status {
            WaitStatus::Exited(_pid, exit_status) => {
                jobs.remove(index);
                ControlFlow::Break(ExitStatus(exit_status))
            }
            WaitStatus::Signaled(_pid, signal, _core_dumped) => {
                jobs.remove(index);
                ControlFlow::Break(ExitStatus::from(signal))
            }
            WaitStatus::Stopped(_pid, signal) if job_control.into() => {
                ControlFlow::Break(ExitStatus::from(signal))
            }
            _ => ControlFlow::Continue(()),
        }
    }
}

/// Returns a closure that tests if any job is running.
///
/// The closure applies [`job_status`] to each job in the job set. If all jobs
/// have finished, the closure returns [`ControlFlow::Break`] with the exit
/// status of 0. Otherwise, the closure returns [`ControlFlow::Continue`].
pub fn any_job_is_running(
    job_control: State,
) -> impl FnMut(&mut JobSet) -> ControlFlow<ExitStatus> {
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
    use yash_env::trap::Signal;

    #[test]
    fn status_of_unknown_job() {
        let mut jobs = JobSet::new();
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
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
        job.status = WaitStatus::Exited(Pid::from_raw(123), 0);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );
        assert_eq!(jobs.get(index), None);

        let mut job = Job::new(Pid::from_raw(456));
        job.status = WaitStatus::Exited(Pid::from_raw(456), 42);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus(42)),
        );
        assert_eq!(jobs.get(index), None);
    }

    #[test]
    fn status_of_signaled_job() {
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
        job.status = WaitStatus::Signaled(Pid::from_raw(123), Signal::SIGHUP, false);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(Signal::SIGHUP)),
        );
        assert_eq!(jobs.get(index), None);

        let mut job = Job::new(Pid::from_raw(456));
        job.status = WaitStatus::Signaled(Pid::from_raw(456), Signal::SIGTERM, true);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(Signal::SIGTERM)),
        );
        assert_eq!(jobs.get(index), None);
    }

    #[test]
    fn status_of_stopped_job_without_job_control() {
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
        job.status = WaitStatus::Stopped(Pid::from_raw(123), Signal::SIGTSTP);
        let index = jobs.add(job);

        assert_eq!(job_status(index, Off)(&mut jobs), ControlFlow::Continue(()),);
        assert_eq!(jobs.get(index).unwrap().pid, Pid::from_raw(123));

        let mut job = Job::new(Pid::from_raw(456));
        job.status = WaitStatus::Stopped(Pid::from_raw(456), Signal::SIGSTOP);
        let index = jobs.add(job);

        assert_eq!(job_status(index, Off)(&mut jobs), ControlFlow::Continue(()),);
        assert_eq!(jobs.get(index).unwrap().pid, Pid::from_raw(456));
    }

    #[test]
    fn status_of_stopped_job_with_job_control() {
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
        job.status = WaitStatus::Stopped(Pid::from_raw(123), Signal::SIGTSTP);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(Signal::SIGTSTP)),
        );
        assert_eq!(jobs.get(index).unwrap().pid, Pid::from_raw(123));

        let mut job = Job::new(Pid::from_raw(456));
        job.status = WaitStatus::Stopped(Pid::from_raw(456), Signal::SIGSTOP);
        let index = jobs.add(job);

        assert_eq!(
            job_status(index, On)(&mut jobs),
            ControlFlow::Break(ExitStatus::from(Signal::SIGSTOP)),
        );
        assert_eq!(jobs.get(index).unwrap().pid, Pid::from_raw(456));
    }

    #[test]
    fn status_of_continued_job() {
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
        job.status = WaitStatus::Continued(Pid::from_raw(123));
        let index = jobs.add(job);

        assert_eq!(job_status(index, Off)(&mut jobs), ControlFlow::Continue(()),);
        assert_eq!(jobs.get(index).unwrap().pid, Pid::from_raw(123));

        let mut job = Job::new(Pid::from_raw(456));
        job.status = WaitStatus::Continued(Pid::from_raw(456));
        let index = jobs.add(job);

        assert_eq!(job_status(index, On)(&mut jobs), ControlFlow::Continue(()),);
        assert_eq!(jobs.get(index).unwrap().pid, Pid::from_raw(456));
    }

    #[test]
    fn status_of_disowned_job() {
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
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
        let mut jobs = JobSet::new();
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
        let mut jobs = JobSet::new();
        let mut job = Job::new(Pid::from_raw(123));
        job.status = WaitStatus::Exited(Pid::from_raw(123), 0);
        jobs.add(job);

        assert_eq!(
            any_job_is_running(Off)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );

        let mut job = Job::new(Pid::from_raw(456));
        job.status = WaitStatus::Exited(Pid::from_raw(456), 42);
        jobs.add(job);

        assert_eq!(
            any_job_is_running(On)(&mut jobs),
            ControlFlow::Break(ExitStatus::SUCCESS),
        );
    }

    #[test]
    fn any_job_is_running_with_running_job() {
        let mut jobs = JobSet::new();
        jobs.add(Job::new(Pid::from_raw(123)));

        assert_eq!(
            any_job_is_running(Off)(&mut jobs),
            ControlFlow::Continue(()),
        );

        jobs.add(Job::new(Pid::from_raw(456)));

        assert_eq!(any_job_is_running(On)(&mut jobs), ControlFlow::Continue(()),);

        // Exited jobs are ignored
        let mut job = Job::new(Pid::from_raw(789));
        job.status = WaitStatus::Exited(Pid::from_raw(789), 0);
        jobs.add(job);

        assert_eq!(any_job_is_running(On)(&mut jobs), ControlFlow::Continue(()),);
    }
}
