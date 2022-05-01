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
//!
//! A [`JobSet`] manages the state of jobs executed by the shell.
//! Each [`Job`] in the job set remembers the latest state of the child process
//! performing the job's task.
//!
//! The job set stores jobs in an internal array. The index of a job in the
//! array never changes once the [job is added](JobSet::add) to the job set.
//! The index of the other jobs does not change when you [remove a
//! job](JobSet::remove). Note that the job set may reuse the index of a removed
//! job for another job added later.
//!
//! When the [wait system call](crate::System::wait) returns a new status of a
//! child process, the caller should pass it to [`JobSet::update_status`], which
//! modifies the status of the corresponding job. The `status_changed` flag of
//! the job is set when the job is updated and should be reset when
//! [reported](JobSet::report_job).
//!
//! The job set remembers the selection of two special jobs called the "current
//! job" and "previous job." The previous job is chosen automatically, so there
//! is no function to modify it. You can change the current job by
//! [`JobSet::set_current_job`].
//!
//! The [`JobSet::set_last_async_pid`] function remembers the process ID of the
//! last executed asynchronous command, which will be the value of the `$!`
//! special parameter.

#[doc(no_inline)]
pub use nix::sys::wait::WaitStatus;
#[doc(no_inline)]
pub use nix::unistd::Pid;
use slab::Slab;
use std::collections::HashMap;
use std::iter::FusedIterator;
use std::ops::Deref;

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

    fn is_suspended(&self) -> bool {
        matches!(self.status, WaitStatus::Stopped(_, _))
    }
}

/// Partially mutable reference to [`Job`].
///
/// This struct is a custom reference type for `Job`. It allows resetting the
/// `status_changed` flag of the referenced job via the
/// [`status_reported`](Self::status_reported) method, but does not provide any
/// other mutability. Note that immutable access to the job is possible with the
/// `Deref` implementation.
#[derive(Debug, Eq, PartialEq)]
pub struct JobRefMut<'a>(&'a mut Job);

impl JobRefMut<'_> {
    /// Clears the `status_changed` flag of the job.
    ///
    /// Normally, this method should be called when the shell printed a job
    /// status report.
    pub fn status_reported(&mut self) {
        self.0.status_changed = false
    }
}

impl Deref for JobRefMut<'_> {
    type Target = Job;
    fn deref(&self) -> &Job {
        self.0
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
///
/// See the [module documentation](self) for details.
#[derive(Clone, Debug)]
pub struct JobSet {
    /// Jobs managed by the shell
    jobs: Slab<Job>,

    /// Map from process IDs to indices of `jobs`
    ///
    /// This is a shortcut to quickly find jobs by process ID.
    pids_to_indices: HashMap<Pid, usize>,

    /// Index of the current job. (Only valid when the set is non-empty)
    current_job_index: usize,

    /// Index of the previous job. (Only valid when the set is non-empty)
    previous_job_index: usize,

    /// Process ID of the most recently executed asynchronous command.
    last_async_pid: Pid,
}

impl Default for JobSet {
    fn default() -> Self {
        JobSet {
            jobs: Slab::new(),
            pids_to_indices: HashMap::new(),
            current_job_index: usize::default(),
            previous_job_index: usize::default(),
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
    ///
    /// If the new job is suspended and the [current job](Self::current_job) is
    /// not, the new job becomes the current job. If the new job and the current
    /// job are suspended but the [previous job](Self::previous_job) is not, the
    /// new job becomes the previous job.
    pub fn add(&mut self, job: Job) -> usize {
        let new_job_is_suspended = job.is_suspended();
        let ex_current_job_is_suspended = self.current_job().map(|(_, job)| job.is_suspended());
        let ex_previous_job_is_suspended = self.previous_job().map(|(_, job)| job.is_suspended());

        // Add the job to `self.jobs` and `self.pids_to_indices`.
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

        // Reselect the current and previous job.
        match ex_current_job_is_suspended {
            None => self.current_job_index = index,
            Some(false) if new_job_is_suspended => self.set_current_job(index).unwrap(),
            Some(_) => match ex_previous_job_is_suspended {
                None => self.previous_job_index = index,
                Some(false) if new_job_is_suspended => self.previous_job_index = index,
                Some(_) => (),
            },
        }

        index
    }

    /// Removes a job from this job set.
    ///
    /// This function returns the job removed from the job set.
    /// The result is `None` if there is no job for the index.
    ///
    /// If the removed job is the [current job](Self::current_job), the
    /// [previous job](Self::previous_job) becomes the current job and another
    /// job is selected for the new previous job, if any.
    /// If the removed job is the previous job, another job is selected for the
    /// new previous job, if any.
    pub fn remove(&mut self, index: usize) -> Option<Job> {
        let job = self.jobs.try_remove(index);

        if let Some(job) = &job {
            // Keep `pids_to_indices` in sync
            self.pids_to_indices.remove(&job.pid);

            if self.jobs.is_empty() {
                // Clearing an already empty slab may seem redundant, but this
                // operation purges the slab's internal cache of unused indices,
                // so that jobs added later have indices starting from 0.
                self.jobs.clear();
            }

            // Reselect the current and previous job
            let previous_job_becomes_current_job = index == self.current_job_index;
            if previous_job_becomes_current_job {
                self.current_job_index = self.previous_job_index;
            }
            if previous_job_becomes_current_job || index == self.previous_job_index {
                self.previous_job_index = self
                    .any_suspended_job_but_current()
                    .unwrap_or_else(|| self.any_job_but_current().unwrap_or_default());
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
        let max_index = match self.jobs.iter().next_back() {
            Some((index, _job)) => index,
            None => return,
        };
        for index in 0..=max_index {
            if let Some(job) = self.get(index) {
                if !f(index, job) {
                    self.remove(index);
                }
            }
        }

        debug_assert_eq!(self.jobs.len(), self.pids_to_indices.len());
    }

    /// Returns the job at the specified index.
    ///
    /// The result is `None` if there is no job for the index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<&Job> {
        self.jobs.get(index)
    }

    /// Returns a partially mutable reference to the job at the specified index.
    ///
    /// The result is `None` if there is no job for the index.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<JobRefMut> {
        self.jobs.get_mut(index).map(JobRefMut)
    }

    /// Returns the number of jobs in this job set.
    #[inline]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Returns true if this job set contains no jobs.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
    /// This function returns the index of the job whose process ID is `pid`.
    /// The result is `None` if no such job is found.
    ///
    /// A `JobSet` maintains an internal hash map to quickly find jobs by
    /// process ID.
    pub fn find_by_pid(&self, pid: Pid) -> Option<usize> {
        self.pids_to_indices.get(&pid).copied()
    }
}

/// Supports indexing operation on `JobSet`.
impl std::ops::Index<usize> for JobSet {
    type Output = Job;

    /// Returns a reference to the specified job.
    ///
    /// This function will panic if the job does not exist.
    fn index(&self, index: usize) -> &Job {
        &self.jobs[index]
    }
}

/// Iterator that conditionally removes jobs from a job set.
///
/// Call [`JobSet::drain_filter`] to get an instance of `DrainFilter`.
#[derive(Debug)]
pub struct DrainFilter<'a, F>
where
    F: FnMut(usize, JobRefMut) -> bool,
{
    set: &'a mut JobSet,
    should_remove: F,
    next_index: usize,
    len: usize,
}

impl<F> Iterator for DrainFilter<'_, F>
where
    F: FnMut(usize, JobRefMut) -> bool,
{
    type Item = (usize, Job);

    fn next(&mut self) -> Option<(usize, Job)> {
        while self.len > 0 {
            let index = self.next_index;
            self.next_index += 1;
            if let Some(job) = self.set.get_mut(index) {
                self.len -= 1;
                if (self.should_remove)(index, job) {
                    let job = self.set.remove(index).unwrap();
                    return Some((index, job));
                }
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.len))
    }
}

impl<F> FusedIterator for DrainFilter<'_, F> where F: FnMut(usize, JobRefMut) -> bool {}

impl<F> Drop for DrainFilter<'_, F>
where
    F: FnMut(usize, JobRefMut) -> bool,
{
    fn drop(&mut self) {
        // Ensure all jobs are processed
        self.for_each(drop)
    }
}

impl JobSet {
    /// Returns an iterator that conditionally modifies and removes jobs.
    ///
    /// The iterator uses the `should_remove` function to decide whether to
    /// remove jobs. The function is called with every index and job reference.
    /// If the function returns true, the job is removed and yielded from the
    /// iterator. Otherwise, the job remains in the set.
    ///
    /// You can reset the `status_changed` flag of a job
    /// ([`JobRefMut::status_reported`]) regardless of whether you choose to
    /// remove it or not.
    ///
    /// Note that the `should_remove` function is called for all the remaining
    /// jobs when the `DrainFilter` is dropped before iterating all jobs.
    pub fn drain_filter<F>(&mut self, should_remove: F) -> DrainFilter<'_, F>
    where
        F: FnMut(usize, JobRefMut) -> bool,
    {
        let len = self.len();
        DrainFilter {
            set: self,
            should_remove,
            next_index: 0,
            len,
        }
    }
}

impl JobSet {
    /// Updates the status of a job.
    ///
    /// The result of a `waitpid` call should be passed to this function.
    /// It updates the status of the job as indicated by `status`, and sets the
    /// `status_changed` flag in the job.
    ///
    /// Returns the index of the job updated. If `status` describes a process
    /// not managed in this job set, the result is `None`.
    ///
    /// When a job is suspended (i.e., `status` is `Stopped`), the job becomes
    /// the [current job](Self::current_job) and the old current job becomes the
    /// [previous job](Self::previous_job). When a suspended job gets a status
    /// update:
    ///
    /// - If the updated job is the current job and the previous job is
    ///   suspended, the previous job becomes the current job and the new
    ///   previous job is chosen from other suspended jobs. If there is no
    ///   suspended jobs, the new previous jobs is the old current job.
    /// - If the updated job is the previous job and there is a suspended job
    ///   other than the current job, it becomes the previous job.
    pub fn update_status(&mut self, status: WaitStatus) -> Option<usize> {
        let pid = status.pid()?;
        let index = self.find_by_pid(pid);
        if let Some(index) = index {
            // Update the job status.
            let job = &mut self.jobs[index];
            let was_suspended = job.is_suspended();
            job.status = status;
            job.status_changed = true;

            // Reselect the current and previous job.
            if !was_suspended && job.is_suspended() {
                if index != self.current_job_index {
                    self.previous_job_index = std::mem::replace(&mut self.current_job_index, index);
                }
            } else if was_suspended && !job.is_suspended() {
                if let Some((prev_index, prev)) = self.previous_job() {
                    let previous_job_becomes_current_job =
                        index == self.current_job_index && prev.is_suspended();
                    if previous_job_becomes_current_job {
                        self.current_job_index = prev_index;
                    }
                    if previous_job_becomes_current_job || index == prev_index {
                        self.previous_job_index =
                            self.any_suspended_job_but_current().unwrap_or(index);
                    }
                }
            }
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

/// Error type for [`JobSet::set_current_job`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SetCurrentJobError {
    /// The specified index does not refer to any job.
    NoSuchJob,
    /// The specified job is not a suspended job and there is another suspended
    /// job.
    NotSuspended,
}

impl JobSet {
    /// Selects the current job.
    ///
    /// This function changes the current job to the job specified by the index
    /// and the previous job to the old current job.
    ///
    /// If there is one or more suspended jobs, the current job must be selected
    /// from them. If the index does not refer to a suspended job, the
    /// `NotSuspended` error is returned.
    ///
    /// If the index does not refer to any job, the `NoSuchJob` error is
    /// returned.
    pub fn set_current_job(&mut self, index: usize) -> Result<(), SetCurrentJobError> {
        let job = self.get(index).ok_or(SetCurrentJobError::NoSuchJob)?;
        if !job.is_suspended() && self.iter().any(|(_, job)| job.is_suspended()) {
            return Err(SetCurrentJobError::NotSuspended);
        }

        if index != self.current_job_index {
            self.previous_job_index = std::mem::replace(&mut self.current_job_index, index);
        }
        Ok(())
    }

    /// Returns the current job.
    ///
    /// If the job set contains at least one job, there is a current job. This
    /// function returns it with its index. If the job set is empty, the result
    /// is `None`.
    ///
    /// If there is any suspended jobs, the current job must be a suspended job.
    /// Running or terminated jobs can be the current job if there is no
    /// suspended job. You can [change the current job](Self::set_current_job)
    /// as long as the above rules are met.
    ///
    /// See also [`previous_job`](Self::previous_job).
    pub fn current_job(&self) -> Option<(usize, &Job)> {
        self.get(self.current_job_index)
            .map(|job| (self.current_job_index, job))
    }

    /// Returns the previous job.
    ///
    /// If the job set contains two or more jobs, there is a previous job. This
    /// function returns it with the index. If the job set has zero or one job,
    /// the result is `None`.
    ///
    /// The previous job is never the same job as the [current
    /// job](Self::current_job).
    ///
    /// If there are two or more suspended jobs, the previous job must be a
    /// suspended job.  Running or terminated jobs can be the previous job if
    /// there is zero or one suspended job.
    ///
    /// You cannot directly select the previous job. When the current job is
    /// selected, the old current job becomes the previous job.
    pub fn previous_job(&self) -> Option<(usize, &Job)> {
        if self.previous_job_index == self.current_job_index {
            None
        } else {
            self.get(self.previous_job_index)
                .map(|job| (self.previous_job_index, job))
        }
    }

    /// Finds a suspended job other than the current job.
    fn any_suspended_job_but_current(&self) -> Option<usize> {
        self.iter()
            .filter(|&(index, job)| index != self.current_job_index && job.is_suspended())
            .map(|(index, _)| index)
            .next()
    }

    /// Finds a job other than the current job.
    fn any_job_but_current(&self) -> Option<usize> {
        self.iter()
            .filter(|&(index, _)| index != self.current_job_index)
            .map(|(index, _)| index)
            .next()
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
    use crate::trap::Signal;

    #[test]
    fn job_set_add_and_remove() {
        // This test case depends on how Slab reuses the index of removed items.
        let mut set = JobSet::default();

        assert_eq!(set.add(Job::new(Pid::from_raw(10))), 0);
        assert_eq!(set.add(Job::new(Pid::from_raw(11))), 1);
        assert_eq!(set.add(Job::new(Pid::from_raw(12))), 2);

        assert_eq!(set.remove(0).unwrap().pid, Pid::from_raw(10));
        assert_eq!(set.remove(1).unwrap().pid, Pid::from_raw(11));

        // Indices are reused in the reverse order of removals.
        assert_eq!(set.add(Job::new(Pid::from_raw(13))), 1);
        assert_eq!(set.add(Job::new(Pid::from_raw(14))), 0);

        assert_eq!(set.remove(0).unwrap().pid, Pid::from_raw(14));
        assert_eq!(set.remove(1).unwrap().pid, Pid::from_raw(13));
        assert_eq!(set.remove(2).unwrap().pid, Pid::from_raw(12));

        // Once the job set is empty, indices start from 0 again.
        assert_eq!(set.add(Job::new(Pid::from_raw(13))), 0);
        assert_eq!(set.add(Job::new(Pid::from_raw(14))), 1);
    }

    #[test]
    fn job_set_add_same_pid() {
        let mut set = JobSet::default();

        let mut job = Job::new(Pid::from_raw(10));
        job.name = "first job".to_string();
        let i_first = set.add(job);

        let mut job = Job::new(Pid::from_raw(10));
        job.name = "second job".to_string();
        let i_second = set.add(job);

        let job = set.get(i_second).unwrap();
        assert_eq!(job.pid, Pid::from_raw(10));
        assert_eq!(job.name, "second job");

        assert_ne!(
            set.get(i_first).map(|job| job.name.as_str()),
            Some("first job")
        );
    }

    #[test]
    fn job_set_retain_jobs() {
        let mut set = JobSet::default();
        set.add(Job::new(Pid::from_raw(4)));
        set.add(Job::new(Pid::from_raw(5)));
        set.add(Job::new(Pid::from_raw(6)));
        set.add(Job::new(Pid::from_raw(7)));
        set.add(Job::new(Pid::from_raw(8)));
        set.add(Job::new(Pid::from_raw(9)));
        set.retain_jobs(|index, job| index != 2 && job.pid != Pid::from_raw(8));
        let mut pids: Vec<_> = set.iter().map(|(index, job)| (index, job.pid)).collect();
        pids.sort_unstable();
        assert_eq!(
            pids,
            [
                (0, Pid::from_raw(4)),
                (1, Pid::from_raw(5)),
                (3, Pid::from_raw(7)),
                (5, Pid::from_raw(9)),
            ]
        );

        let mut set = JobSet::default();
        set.add(Job::new(Pid::from_raw(17)));
        set.add(Job::new(Pid::from_raw(18)));
        set.add(Job::new(Pid::from_raw(19)));
        set.add(Job::new(Pid::from_raw(20)));
        set.add(Job::new(Pid::from_raw(21)));
        set.retain_jobs(|_index, job| job.pid.as_raw() % 2 == 0);
        let mut pids: Vec<_> = set.iter().map(|(index, job)| (index, job.pid)).collect();
        pids.sort_unstable();
        assert_eq!(pids, [(1, Pid::from_raw(18)), (3, Pid::from_raw(20))]);
    }

    #[test]
    fn job_set_find_by_pid() {
        let mut set = JobSet::default();
        assert_eq!(set.find_by_pid(Pid::from_raw(10)), None);

        let i10 = set.add(Job::new(Pid::from_raw(10)));
        let i20 = set.add(Job::new(Pid::from_raw(20)));
        let i30 = set.add(Job::new(Pid::from_raw(30)));
        assert_eq!(set.find_by_pid(Pid::from_raw(10)), Some(i10));
        assert_eq!(set.find_by_pid(Pid::from_raw(20)), Some(i20));
        assert_eq!(set.find_by_pid(Pid::from_raw(30)), Some(i30));
        assert_eq!(set.find_by_pid(Pid::from_raw(40)), None);

        set.remove(i10);
        assert_eq!(set.find_by_pid(Pid::from_raw(10)), None);
    }

    #[test]
    fn job_set_drain_filter_consumed() {
        let mut set = JobSet::default();
        let i21 = set.add(Job::new(Pid::from_raw(21)));
        let i22 = set.add(Job::new(Pid::from_raw(22)));
        let i23 = set.add(Job::new(Pid::from_raw(23)));
        let i24 = set.add(Job::new(Pid::from_raw(24)));
        let i25 = set.add(Job::new(Pid::from_raw(25)));
        let i26 = set.add(Job::new(Pid::from_raw(26)));
        set.remove(i23).unwrap();

        let mut i = set.drain_filter(|index, mut job| {
            assert_ne!(index, i23);
            if index % 2 == 0 {
                job.status_reported();
            }
            index == 0 || job.pid == Pid::from_raw(26)
        });
        let mut j21 = Job::new(Pid::from_raw(21));
        j21.status_changed = false;
        assert_eq!(i.next(), Some((i21, j21)));
        assert_eq!(i.next(), Some((i26, Job::new(Pid::from_raw(26)))));
        assert_eq!(i.next(), None);
        assert_eq!(i.next(), None); // The drain filter is fused.
        drop(i);

        let indices: Vec<usize> = set.iter().map(|(index, _)| index).collect();
        assert_eq!(indices, [i22, i24, i25]);
        assert!(!set[i25].status_changed);
    }

    #[test]
    fn job_set_drain_filter_unconsumed() {
        let mut set = JobSet::default();
        let _i21 = set.add(Job::new(Pid::from_raw(21)));
        let i22 = set.add(Job::new(Pid::from_raw(22)));
        let i23 = set.add(Job::new(Pid::from_raw(23)));
        let i24 = set.add(Job::new(Pid::from_raw(24)));
        let i25 = set.add(Job::new(Pid::from_raw(25)));
        let _i26 = set.add(Job::new(Pid::from_raw(26)));
        set.remove(i23).unwrap();

        let i = set.drain_filter(|index, mut job| {
            if index % 2 == 0 {
                job.status_reported();
            }
            index == 0 || job.pid == Pid::from_raw(26)
        });
        drop(i);

        let indices: Vec<usize> = set.iter().map(|(index, _)| index).collect();
        assert_eq!(indices, [i22, i24, i25]);
        assert!(!set[i25].status_changed);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn updating_job_status() {
        let mut set = JobSet::default();
        let status = WaitStatus::Exited(Pid::from_raw(20), 15);
        assert_eq!(set.update_status(status), None);

        let i10 = set.add(Job::new(Pid::from_raw(10)));
        let i20 = set.add(Job::new(Pid::from_raw(20)));
        let i30 = set.add(Job::new(Pid::from_raw(30)));
        assert_eq!(set.get(i20).unwrap().status, WaitStatus::StillAlive);

        set.report_job(i20, |_| true);

        let i20_2 = set.update_status(status);
        assert_eq!(i20_2, Some(i20));
        assert_eq!(set.get(i20).unwrap().status, status);
        assert_eq!(set.get(i20).unwrap().status_changed, true);

        assert_eq!(set.get(i10).unwrap().status, WaitStatus::StillAlive);
        assert_eq!(set.get(i30).unwrap().status, WaitStatus::StillAlive);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn job_set_report_job() {
        let mut set = JobSet::default();
        set.report_job(0, |_| unreachable!());

        let i5 = set.add(Job::new(Pid::from_raw(5)));
        set.report_job(i5, |job| {
            assert_eq!(job.status_changed, true);
            false
        });
        assert_eq!(set.get(i5).unwrap().status_changed, true);
        set.report_job(i5, |job| {
            assert_eq!(job.status_changed, true);
            true
        });
        assert_eq!(set.get(i5).unwrap().status_changed, false);
        set.report_job(i5, |job| {
            assert_eq!(job.status_changed, false);
            true
        });
        assert_eq!(set.get(i5).unwrap().status_changed, false);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn job_set_report_jobs() {
        let mut set = JobSet::default();
        set.report_jobs(|_, _| unreachable!());

        let i5 = set.add(Job::new(Pid::from_raw(5)));
        let i7 = set.add(Job::new(Pid::from_raw(7)));
        let i9 = set.add(Job::new(Pid::from_raw(9)));
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
        assert_eq!(set.get(i5).unwrap().status_changed, true);
        assert_eq!(set.get(i7).unwrap().status_changed, false);
        assert_eq!(set.get(i9).unwrap().status_changed, true);
    }

    #[test]
    fn no_current_and_previous_job_in_empty_job_set() {
        let set = JobSet::default();
        assert_eq!(set.current_job(), None);
        assert_eq!(set.previous_job(), None);
    }

    #[test]
    fn current_and_previous_job_in_job_set_with_one_job() {
        let mut set = JobSet::default();
        let job = Job::new(Pid::from_raw(10));
        let i10 = set.add(job.clone());
        assert_eq!(set.current_job(), Some((i10, &job)));
        assert_eq!(set.previous_job(), None);
    }

    #[test]
    fn current_and_previous_job_in_job_set_with_two_job() {
        // If one job is suspended and the other is not, the current job is the
        // suspended one.
        let mut set = JobSet::default();
        let mut suspended = Job::new(Pid::from_raw(10));
        suspended.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGSTOP);
        let running = Job::new(Pid::from_raw(20));
        let i10 = set.add(suspended.clone());
        let i20 = set.add(running.clone());
        assert_eq!(set.current_job(), Some((i10, &suspended)));
        assert_eq!(set.previous_job(), Some((i20, &running)));

        // The order of adding jobs does not matter in this case.
        set = JobSet::default();
        let i20 = set.add(running.clone());
        let i10 = set.add(suspended.clone());
        assert_eq!(set.current_job(), Some((i10, &suspended)));
        assert_eq!(set.previous_job(), Some((i20, &running)));
    }

    #[test]
    fn adding_suspended_job_with_running_current_and_previous_job() {
        let mut set = JobSet::default();
        let running_1 = Job::new(Pid::from_raw(11));
        let running_2 = Job::new(Pid::from_raw(12));
        set.add(running_1);
        set.add(running_2);
        let ex_current_job_index = set.current_job().unwrap().0;
        let ex_previous_job_index = set.previous_job().unwrap().0;
        assert_ne!(ex_current_job_index, ex_previous_job_index);

        let mut suspended = Job::new(Pid::from_raw(20));
        suspended.status = WaitStatus::Stopped(Pid::from_raw(20), Signal::SIGSTOP);
        let i20 = set.add(suspended);
        let now_current_job_index = set.current_job().unwrap().0;
        let now_previous_job_index = set.previous_job().unwrap().0;
        assert_eq!(now_current_job_index, i20);
        assert_eq!(now_previous_job_index, ex_current_job_index);
    }

    #[test]
    fn adding_suspended_job_with_suspended_current_and_running_previous_job() {
        let mut set = JobSet::default();

        let running = Job::new(Pid::from_raw(18));
        let i18 = set.add(running);

        let mut suspended_1 = Job::new(Pid::from_raw(19));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(19), Signal::SIGSTOP);
        let i19 = set.add(suspended_1);

        let ex_current_job_index = set.current_job().unwrap().0;
        let ex_previous_job_index = set.previous_job().unwrap().0;
        assert_eq!(ex_current_job_index, i19);
        assert_eq!(ex_previous_job_index, i18);

        let mut suspended_2 = Job::new(Pid::from_raw(20));
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(20), Signal::SIGSTOP);
        let i20 = set.add(suspended_2);

        let now_current_job_index = set.current_job().unwrap().0;
        let now_previous_job_index = set.previous_job().unwrap().0;
        assert_eq!(now_current_job_index, ex_current_job_index);
        assert_eq!(now_previous_job_index, i20);
    }

    #[test]
    fn removing_current_job() {
        let mut set = JobSet::default();

        let running = Job::new(Pid::from_raw(10));
        let i10 = set.add(running);

        let mut suspended_1 = Job::new(Pid::from_raw(11));
        let mut suspended_2 = Job::new(Pid::from_raw(12));
        let mut suspended_3 = Job::new(Pid::from_raw(13));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(11), Signal::SIGSTOP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGSTOP);
        suspended_3.status = WaitStatus::Stopped(Pid::from_raw(13), Signal::SIGSTOP);
        set.add(suspended_1);
        set.add(suspended_2);
        set.add(suspended_3);

        let current_job_index_1 = set.current_job().unwrap().0;
        let previous_job_index_1 = set.previous_job().unwrap().0;
        assert_ne!(current_job_index_1, i10);
        assert_ne!(previous_job_index_1, i10);

        set.remove(current_job_index_1);
        let current_job_index_2 = set.current_job().unwrap().0;
        let (previous_job_index_2, previous_job_2) = set.previous_job().unwrap();
        assert_eq!(current_job_index_2, previous_job_index_1);
        assert_ne!(previous_job_index_2, current_job_index_2);
        // The new previous job is chosen from suspended jobs other than the current job.
        assert!(previous_job_2.is_suspended(), "{:?}", previous_job_2);

        set.remove(current_job_index_2);
        let current_job_index_3 = set.current_job().unwrap().0;
        let previous_job_index_3 = set.previous_job().unwrap().0;
        assert_eq!(current_job_index_3, previous_job_index_2);
        // There is no other suspended job, so the new previous job is a running job.
        assert_eq!(previous_job_index_3, i10);

        set.remove(current_job_index_3);
        let current_job_index_4 = set.current_job().unwrap().0;
        assert_eq!(current_job_index_4, i10);
        // No more job to be selected for the previous job.
        assert_eq!(set.previous_job(), None);
    }

    #[test]
    fn removing_previous_job_with_suspended_job() {
        let mut set = JobSet::default();

        let running = Job::new(Pid::from_raw(10));
        let i10 = set.add(running);

        let mut suspended_1 = Job::new(Pid::from_raw(11));
        let mut suspended_2 = Job::new(Pid::from_raw(12));
        let mut suspended_3 = Job::new(Pid::from_raw(13));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(11), Signal::SIGSTOP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGSTOP);
        suspended_3.status = WaitStatus::Stopped(Pid::from_raw(13), Signal::SIGSTOP);
        set.add(suspended_1);
        set.add(suspended_2);
        set.add(suspended_3);

        let ex_current_job_index = set.current_job().unwrap().0;
        let ex_previous_job_index = set.previous_job().unwrap().0;
        assert_ne!(ex_current_job_index, i10);
        assert_ne!(ex_previous_job_index, i10);

        set.remove(ex_previous_job_index);
        let now_current_job_index = set.current_job().unwrap().0;
        let (now_previous_job_index, now_previous_job) = set.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_current_job_index);
        assert_ne!(now_previous_job_index, now_current_job_index);
        // The new previous job is chosen from suspended jobs other than the current job.
        assert!(now_previous_job.is_suspended(), "{:?}", now_previous_job);
    }

    #[test]
    fn removing_previous_job_with_running_job() {
        let mut set = JobSet::default();

        let running = Job::new(Pid::from_raw(10));
        let i10 = set.add(running);

        let mut suspended_1 = Job::new(Pid::from_raw(11));
        let mut suspended_2 = Job::new(Pid::from_raw(12));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(11), Signal::SIGSTOP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGSTOP);
        set.add(suspended_1);
        set.add(suspended_2);

        let ex_current_job_index = set.current_job().unwrap().0;
        let ex_previous_job_index = set.previous_job().unwrap().0;
        assert_ne!(ex_current_job_index, i10);
        assert_ne!(ex_previous_job_index, i10);

        set.remove(ex_previous_job_index);
        let now_current_job_index = set.current_job().unwrap().0;
        let now_previous_job_index = set.previous_job().unwrap().0;
        assert_eq!(now_current_job_index, ex_current_job_index);
        // When there is no suspended job other than the current job,
        // then the new previous job can be any job other than the current.
        assert_eq!(now_previous_job_index, i10);
    }

    #[test]
    fn set_current_job_with_running_jobs_only() {
        let mut set = JobSet::default();
        let i21 = set.add(Job::new(Pid::from_raw(21)));
        let i22 = set.add(Job::new(Pid::from_raw(22)));

        assert_eq!(set.set_current_job(i21), Ok(()));
        assert_eq!(set.current_job().unwrap().0, i21);

        assert_eq!(set.set_current_job(i22), Ok(()));
        assert_eq!(set.current_job().unwrap().0, i22);
    }

    #[test]
    fn set_current_job_to_suspended_job() {
        let mut set = JobSet::default();
        set.add(Job::new(Pid::from_raw(20)));

        let mut suspended_1 = Job::new(Pid::from_raw(21));
        let mut suspended_2 = Job::new(Pid::from_raw(22));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(21), Signal::SIGSTOP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(22), Signal::SIGSTOP);
        let i21 = set.add(suspended_1);
        let i22 = set.add(suspended_2);

        assert_eq!(set.set_current_job(i21), Ok(()));
        assert_eq!(set.current_job().unwrap().0, i21);

        assert_eq!(set.set_current_job(i22), Ok(()));
        assert_eq!(set.current_job().unwrap().0, i22);
    }

    #[test]
    fn set_current_job_no_such_job() {
        let mut set = JobSet::default();
        assert_eq!(set.set_current_job(0), Err(SetCurrentJobError::NoSuchJob));
        assert_eq!(set.set_current_job(1), Err(SetCurrentJobError::NoSuchJob));
        assert_eq!(set.set_current_job(2), Err(SetCurrentJobError::NoSuchJob));
    }

    #[test]
    fn set_current_job_not_suspended() {
        let mut set = JobSet::default();
        let mut suspended = Job::new(Pid::from_raw(10));
        suspended.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTSTP);
        let running = Job::new(Pid::from_raw(20));
        let i10 = set.add(suspended);
        let i20 = set.add(running);
        assert_eq!(
            set.set_current_job(i20),
            Err(SetCurrentJobError::NotSuspended)
        );
        assert_eq!(set.current_job().unwrap().0, i10);
    }

    #[test]
    fn set_current_job_no_change() {
        let mut set = JobSet::default();
        set.add(Job::new(Pid::from_raw(5)));
        set.add(Job::new(Pid::from_raw(6)));
        let old_current_job_index = set.current_job().unwrap().0;
        let old_previous_job_index = set.previous_job().unwrap().0;
        set.set_current_job(old_current_job_index).unwrap();
        let new_current_job_index = set.current_job().unwrap().0;
        let new_previous_job_index = set.previous_job().unwrap().0;
        assert_eq!(new_current_job_index, old_current_job_index);
        assert_eq!(new_previous_job_index, old_previous_job_index);
    }

    #[test]
    fn resuming_current_job_without_other_suspended_jobs() {
        let mut set = JobSet::default();
        let mut suspended = Job::new(Pid::from_raw(10));
        suspended.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTSTP);
        let running = Job::new(Pid::from_raw(20));
        let i10 = set.add(suspended);
        let i20 = set.add(running);
        set.update_status(WaitStatus::Continued(Pid::from_raw(10)));
        assert_eq!(set.current_job().unwrap().0, i10);
        assert_eq!(set.previous_job().unwrap().0, i20);
    }

    #[test]
    fn resuming_current_job_with_another_suspended_job() {
        let mut set = JobSet::default();
        let mut suspended_1 = Job::new(Pid::from_raw(10));
        let mut suspended_2 = Job::new(Pid::from_raw(20));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTSTP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(20), Signal::SIGTSTP);
        let i10 = set.add(suspended_1);
        let i20 = set.add(suspended_2);
        set.set_current_job(i10).unwrap();
        set.update_status(WaitStatus::Continued(Pid::from_raw(10)));
        // The current job must be a suspended job, if any.
        assert_eq!(set.current_job().unwrap().0, i20);
        assert_eq!(set.previous_job().unwrap().0, i10);
    }

    #[test]
    fn resuming_current_job_with_other_suspended_jobs() {
        let mut set = JobSet::default();
        let mut suspended_1 = Job::new(Pid::from_raw(10));
        let mut suspended_2 = Job::new(Pid::from_raw(20));
        let mut suspended_3 = Job::new(Pid::from_raw(30));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTSTP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(20), Signal::SIGTSTP);
        suspended_3.status = WaitStatus::Stopped(Pid::from_raw(30), Signal::SIGTSTP);
        set.add(suspended_1);
        set.add(suspended_2);
        set.add(suspended_3);
        let ex_current_job_pid = set.current_job().unwrap().1.pid;
        let ex_previous_job_index = set.previous_job().unwrap().0;

        set.update_status(WaitStatus::Continued(ex_current_job_pid));
        let now_current_job_index = set.current_job().unwrap().0;
        let (now_previous_job_index, now_previous_job) = set.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_previous_job_index);
        assert_ne!(now_previous_job_index, now_current_job_index);
        // The new previous job is chosen from suspended jobs other than the current job.
        assert!(now_previous_job.is_suspended(), "{:?}", now_previous_job);
    }

    #[test]
    fn resuming_previous_job() {
        let mut set = JobSet::default();
        let mut suspended_1 = Job::new(Pid::from_raw(10));
        let mut suspended_2 = Job::new(Pid::from_raw(20));
        let mut suspended_3 = Job::new(Pid::from_raw(30));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTSTP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(20), Signal::SIGTSTP);
        suspended_3.status = WaitStatus::Stopped(Pid::from_raw(30), Signal::SIGTSTP);
        set.add(suspended_1);
        set.add(suspended_2);
        set.add(suspended_3);
        let ex_current_job_index = set.current_job().unwrap().0;
        let ex_previous_job_pid = set.previous_job().unwrap().1.pid;

        set.update_status(WaitStatus::Continued(ex_previous_job_pid));
        let now_current_job_index = set.current_job().unwrap().0;
        let (now_previous_job_index, now_previous_job) = set.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_current_job_index);
        assert_ne!(now_previous_job_index, now_current_job_index);
        // The new previous job is chosen from suspended jobs other than the current job.
        assert!(now_previous_job.is_suspended(), "{:?}", now_previous_job);
    }

    #[test]
    fn resuming_other_job() {
        let mut set = JobSet::default();
        let mut suspended_1 = Job::new(Pid::from_raw(10));
        let mut suspended_2 = Job::new(Pid::from_raw(20));
        let mut suspended_3 = Job::new(Pid::from_raw(30));
        suspended_1.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTSTP);
        suspended_2.status = WaitStatus::Stopped(Pid::from_raw(20), Signal::SIGTSTP);
        suspended_3.status = WaitStatus::Stopped(Pid::from_raw(30), Signal::SIGTSTP);
        let i10 = set.add(suspended_1);
        let i20 = set.add(suspended_2);
        let _i30 = set.add(suspended_3);
        set.set_current_job(i20).unwrap();
        set.set_current_job(i10).unwrap();
        set.update_status(WaitStatus::Continued(Pid::from_raw(30)));
        assert_eq!(set.current_job().unwrap().0, i10);
        assert_eq!(set.previous_job().unwrap().0, i20);
    }

    #[test]
    fn suspending_current_job() {
        let mut set = JobSet::default();
        let i11 = set.add(Job::new(Pid::from_raw(11)));
        let i12 = set.add(Job::new(Pid::from_raw(12)));
        set.set_current_job(i11).unwrap();
        set.update_status(WaitStatus::Stopped(Pid::from_raw(11), Signal::SIGTTOU));
        assert_eq!(set.current_job().unwrap().0, i11);
        assert_eq!(set.previous_job().unwrap().0, i12);
    }

    #[test]
    fn suspending_previous_job() {
        let mut set = JobSet::default();
        let i11 = set.add(Job::new(Pid::from_raw(11)));
        let i12 = set.add(Job::new(Pid::from_raw(12)));
        set.set_current_job(i11).unwrap();
        set.update_status(WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGTTOU));
        assert_eq!(set.current_job().unwrap().0, i12);
        assert_eq!(set.previous_job().unwrap().0, i11);
    }

    #[test]
    fn suspending_job_with_running_current_job() {
        let mut set = JobSet::default();
        let i10 = set.add(Job::new(Pid::from_raw(10)));
        let _i11 = set.add(Job::new(Pid::from_raw(11)));
        let i12 = set.add(Job::new(Pid::from_raw(12)));
        set.set_current_job(i10).unwrap();
        set.update_status(WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGTTIN));
        assert_eq!(set.current_job().unwrap().0, i12);
        assert_eq!(set.previous_job().unwrap().0, i10);
    }

    #[test]
    fn suspending_job_with_running_previous_job() {
        let mut set = JobSet::default();
        let i11 = set.add(Job::new(Pid::from_raw(11)));
        let i12 = set.add(Job::new(Pid::from_raw(12)));
        let mut suspended = Job::new(Pid::from_raw(10));
        suspended.status = WaitStatus::Stopped(Pid::from_raw(10), Signal::SIGTTIN);
        let i10 = set.add(suspended);
        assert_eq!(set.current_job().unwrap().0, i10);
        assert_eq!(set.previous_job().unwrap().0, i11);

        set.update_status(WaitStatus::Stopped(Pid::from_raw(12), Signal::SIGTTOU));
        assert_eq!(set.current_job().unwrap().0, i12);
        assert_eq!(set.previous_job().unwrap().0, i10);
    }
}
