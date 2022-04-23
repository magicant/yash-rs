// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Type definitions for job management.

#[doc(no_inline)]
pub use nix::sys::wait::WaitStatus;
#[doc(no_inline)]
pub use nix::unistd::Pid;
use slab::Slab;
use std::collections::HashMap;
use std::iter::FusedIterator;

/// Set of one or more processes executing a pipeline
///
/// In the current implementation, a job contains the process ID of one child
/// process of the shell. Though there may be more processes involved in the
/// execution of the pipeline, the shell takes care of only one process of the
/// job.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Job {
    /// Process ID
    pub pid: Pid,

    /// Whether the job is job-controlled.
    ///
    /// If the job is job-controlled, the job process runs in its own process
    /// group.
    pub job_controlled: bool,

    /// Status of the process
    pub status: WaitStatus,

    /// Indicator of status change
    ///
    /// This flag is true if the `status` has been changed since the status was
    /// last reported to the user.
    pub status_changed: bool,

    /// String representation of this process
    pub name: String,
}

impl Job {
    /// Creates a new job instance.
    ///
    /// This function requires a process ID to initialize the new job. The other
    /// members of the job are defaulted.
    pub fn new(pid: Pid) -> Self {
        Job {
            pid,
            job_controlled: false,
            status: WaitStatus::StillAlive,
            status_changed: true,
            name: String::new(),
        }
    }
}

/// Iterator of jobs with indices.
///
/// Call [`JobSet::iter`] to get an instance of `Iter`.
#[derive(Clone, Debug)]
pub struct Iter<'a>(slab::Iter<'a, Job>);

impl<'a> Iterator for Iter<'a> {
    type Item = (usize, &'a Job);

    #[inline(always)]
    fn next(&mut self) -> Option<(usize, &'a Job)> {
        self.0.next()
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a> DoubleEndedIterator for Iter<'a> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<(usize, &'a Job)> {
        self.0.next_back()
    }
}

impl ExactSizeIterator for Iter<'_> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl FusedIterator for Iter<'_> {}

/// Collection of jobs.
#[derive(Clone, Debug)]
pub struct JobSet {
    /// Jobs managed by the shell
    jobs: Slab<Job>,

    /// Map from process IDs to indices of `jobs`
    ///
    /// This is a shortcut to quickly find jobs by process ID.
    pids_to_indices: HashMap<Pid, usize>,

    /// Process ID of the most recently executed asynchronous command.
    last_async_pid: Pid,
}

impl Default for JobSet {
    fn default() -> Self {
        JobSet {
            jobs: Slab::new(),
            pids_to_indices: HashMap::new(),
            last_async_pid: Pid::from_raw(0),
        }
    }
}

impl JobSet {
    /// Adds a job to this job set.
    ///
    /// This function returns a unique index assigned to the job.
    ///
    /// If there already is a job that has the same process ID as that of the
    /// new job, the existing job is silently removed.
    pub fn add_job(&mut self, job: Job) -> usize {
        use std::collections::hash_map::Entry::*;
        let index = match self.pids_to_indices.entry(job.pid) {
            Vacant(entry) => {
                let index = self.jobs.insert(job);
                entry.insert(index);
                index
            }
            Occupied(entry) => {
                let index = *entry.get();
                self.jobs[index] = job;
                index
            }
        };
        debug_assert_eq!(self.jobs.len(), self.pids_to_indices.len());
        index
    }

    /// Removes a job from this job set.
    ///
    /// This function returns the job removed from the job set.
    /// The result is `None` if there is no job for the index.
    pub fn remove_job(&mut self, index: usize) -> Option<Job> {
        let job = self.jobs.try_remove(index);

        if let Some(job) = &job {
            self.pids_to_indices.remove(&job.pid);

            if self.jobs.is_empty() {
                // Clearing an already empty slab may seem redundant, but this
                // operation purges the slab's internal cache of unused indices,
                // so that jobs added later have indices starting from 0.
                self.jobs.clear();
            }
        }
        debug_assert_eq!(self.jobs.len(), self.pids_to_indices.len());

        job
    }

    /// Conditionally removes jobs from this job set.
    ///
    /// Function `f` is called repeatedly with a job and its index.
    /// The job is removed if `f` returns false.
    pub fn retain_jobs<F>(&mut self, mut f: F)
    where
        F: FnMut(usize, &Job) -> bool,
    {
        self.jobs.retain(|index, job| f(index, job))
    }

    /// Returns the job at the specified index.
    ///
    /// The result is `None` if there is no job for the index.
    #[inline]
    pub fn get_job(&self, index: usize) -> Option<&Job> {
        self.jobs.get(index)
    }

    /// Returns the number of jobs in this job set.
    #[inline]
    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    /// Returns true if this job set contains no jobs.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.job_count() == 0
    }

    /// Returns an iterator of jobs with indices.
    ///
    /// The item type of the returned iterator is `(usize, &Job)`.
    /// Jobs are iterated in the order of indices.
    #[inline]
    pub fn iter(&self) -> Iter {
        Iter(self.jobs.iter())
    }

    /// Finds a job by the process ID.
    ///
    /// This function returns the index of the job that contains a process whose
    /// process ID is `pid`. The result is `None` if no such job is found.
    pub fn job_index_by_pid(&self, pid: Pid) -> Option<usize> {
        self.pids_to_indices.get(&pid).copied()
    }
}

impl JobSet {
    /// Updates the status of a job.
    ///
    /// The result of a `waitpid` call should be passed to this function.
    /// It updates the status of the job as indicated by `status`.
    ///
    /// Returns the index of the job updated. If `status` describes a process
    /// not managed in this job set, the result is `None`.
    pub fn update_job(&mut self, status: WaitStatus) -> Option<usize> {
        let pid = status.pid()?;
        let index = self.job_index_by_pid(pid);
        if let Some(index) = index {
            let job = &mut self.jobs[index];
            job.status = status;
            job.status_changed = true;
        }
        index
    }

    /// Examines a job and optionally clears the `status_changed` flag.
    ///
    /// This function passes a reference to the job at the given index to
    /// function `f`. If `f` returns true, the `status_changed` flag is cleared.
    ///
    /// `f` is not called if there is no job at the index.
    ///
    /// Note: Use [`report_jobs`](Self::report_jobs) to operate on all jobs in
    /// the job set.
    pub fn report_job<F>(&mut self, index: usize, f: F)
    where
        F: FnOnce(&Job) -> bool,
    {
        if let Some(job) = self.jobs.get_mut(index) {
            if f(job) {
                job.status_changed = false;
            }
        }
    }

    /// Iterates over jobs and optionally clears the `status_changed` flag.
    ///
    /// This function calls function `f` with a reference to each job in this
    /// job set. If `f` returns true, the job's `status_changed` flag is
    /// cleared.
    ///
    /// Note: Use [`report_job`](Self::report_job) to operate on a single job.
    pub fn report_jobs<F>(&mut self, mut f: F)
    where
        F: FnMut(usize, &Job) -> bool,
    {
        for (index, job) in &mut self.jobs {
            if f(index, job) {
                job.status_changed = false;
            }
        }
    }
}

impl JobSet {
    /// Returns the process ID of the most recently executed asynchronous
    /// command.
    ///
    /// This function returns the value that has been set by
    /// [`set_last_async_pid`](Self::set_last_async_pid), or 0 if no value has
    /// been set.
    pub fn last_async_pid(&self) -> Pid {
        self.last_async_pid
    }

    /// Sets the process ID of the most recently executed asynchronous command.
    ///
    /// This function affects the result of
    /// [`last_async_pid`](Self::last_async_pid).
    pub fn set_last_async_pid(&mut self, pid: Pid) {
        self.last_async_pid = pid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_set_add_and_remove_job() {
        // This test case depends on how Slab reuses the index of removed items.
        let mut set = JobSet::default();

        assert_eq!(set.add_job(Job::new(Pid::from_raw(10))), 0);
        assert_eq!(set.add_job(Job::new(Pid::from_raw(11))), 1);
        assert_eq!(set.add_job(Job::new(Pid::from_raw(12))), 2);

        assert_eq!(set.remove_job(0).unwrap().pid, Pid::from_raw(10));
        assert_eq!(set.remove_job(1).unwrap().pid, Pid::from_raw(11));

        // Indices are reused in the reverse order of removals.
        assert_eq!(set.add_job(Job::new(Pid::from_raw(13))), 1);
        assert_eq!(set.add_job(Job::new(Pid::from_raw(14))), 0);

        assert_eq!(set.remove_job(0).unwrap().pid, Pid::from_raw(14));
        assert_eq!(set.remove_job(1).unwrap().pid, Pid::from_raw(13));
        assert_eq!(set.remove_job(2).unwrap().pid, Pid::from_raw(12));

        // Once the job set is empty, indices start from 0 again.
        assert_eq!(set.add_job(Job::new(Pid::from_raw(13))), 0);
        assert_eq!(set.add_job(Job::new(Pid::from_raw(14))), 1);
    }

    #[test]
    fn job_set_add_job_same_pid() {
        let mut set = JobSet::default();

        let mut job = Job::new(Pid::from_raw(10));
        job.name = "first job".to_string();
        let i_first = set.add_job(job);

        let mut job = Job::new(Pid::from_raw(10));
        job.name = "second job".to_string();
        let i_second = set.add_job(job);

        let job = set.get_job(i_second).unwrap();
        assert_eq!(job.pid, Pid::from_raw(10));
        assert_eq!(job.name, "second job");

        assert_ne!(
            set.get_job(i_first).map(|job| job.name.as_str()),
            Some("first job")
        );
    }

    #[test]
    fn job_set_job_index_by_pid() {
        let mut set = JobSet::default();
        assert_eq!(set.job_index_by_pid(Pid::from_raw(10)), None);

        let i10 = set.add_job(Job::new(Pid::from_raw(10)));
        let i20 = set.add_job(Job::new(Pid::from_raw(20)));
        let i30 = set.add_job(Job::new(Pid::from_raw(30)));
        assert_eq!(set.job_index_by_pid(Pid::from_raw(10)), Some(i10));
        assert_eq!(set.job_index_by_pid(Pid::from_raw(20)), Some(i20));
        assert_eq!(set.job_index_by_pid(Pid::from_raw(30)), Some(i30));
        assert_eq!(set.job_index_by_pid(Pid::from_raw(40)), None);

        set.remove_job(i10);
        assert_eq!(set.job_index_by_pid(Pid::from_raw(10)), None);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn job_set_update_job() {
        let mut set = JobSet::default();
        let status = WaitStatus::Exited(Pid::from_raw(20), 15);
        assert_eq!(set.update_job(status), None);

        let i10 = set.add_job(Job::new(Pid::from_raw(10)));
        let i20 = set.add_job(Job::new(Pid::from_raw(20)));
        let i30 = set.add_job(Job::new(Pid::from_raw(30)));
        assert_eq!(set.get_job(i20).unwrap().status, WaitStatus::StillAlive);

        set.report_job(i20, |_| true);

        let i20_2 = set.update_job(status);
        assert_eq!(i20_2, Some(i20));
        assert_eq!(set.get_job(i20).unwrap().status, status);
        assert_eq!(set.get_job(i20).unwrap().status_changed, true);

        assert_eq!(set.get_job(i10).unwrap().status, WaitStatus::StillAlive);
        assert_eq!(set.get_job(i30).unwrap().status, WaitStatus::StillAlive);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn job_set_report_job() {
        let mut set = JobSet::default();
        set.report_job(0, |_| unreachable!());

        let i5 = set.add_job(Job::new(Pid::from_raw(5)));
        set.report_job(i5, |job| {
            assert_eq!(job.status_changed, true);
            false
        });
        assert_eq!(set.get_job(i5).unwrap().status_changed, true);
        set.report_job(i5, |job| {
            assert_eq!(job.status_changed, true);
            true
        });
        assert_eq!(set.get_job(i5).unwrap().status_changed, false);
        set.report_job(i5, |job| {
            assert_eq!(job.status_changed, false);
            true
        });
        assert_eq!(set.get_job(i5).unwrap().status_changed, false);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn job_set_report_jobs() {
        let mut set = JobSet::default();
        set.report_jobs(|_, _| unreachable!());

        let i5 = set.add_job(Job::new(Pid::from_raw(5)));
        let i7 = set.add_job(Job::new(Pid::from_raw(7)));
        let i9 = set.add_job(Job::new(Pid::from_raw(9)));
        let mut args = Vec::new();
        set.report_jobs(|index, job| {
            args.push((index, job.pid));
            index == i7
        });
        assert_eq!(
            args,
            [
                (i5, Pid::from_raw(5)),
                (i7, Pid::from_raw(7)),
                (i9, Pid::from_raw(9)),
            ]
        );
        assert_eq!(set.get_job(i5).unwrap().status_changed, true);
        assert_eq!(set.get_job(i7).unwrap().status_changed, false);
        assert_eq!(set.get_job(i9).unwrap().status_changed, true);
    }
}
