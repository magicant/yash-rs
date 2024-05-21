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
//! A [`JobList`] manages the state of jobs executed by the shell.
//! Each [`Job`] in the job list remembers the latest state of the child process
//! performing the job's task.
//!
//! The job list stores jobs in an internal array. The index of a job in the
//! array never changes once the [job is added](JobList::add) to the job list.
//! The index of the other jobs does not change when you [remove a
//! job](JobList::remove). Note that the job list may reuse the index of a
//! removed job for another job added later.
//!
//! When the [wait system call](crate::System::wait) returns a new state of a
//! child process, the caller should pass it to [`JobList::update_status`],
//! which modifies the state of the corresponding job. The `state_changed` flag
//! of the job is set when the job is updated and should be
//! [reset when reported](JobRefMut::state_reported).
//!
//! The job list remembers the selection of two special jobs called the "current
//! job" and "previous job." The previous job is chosen automatically, so there
//! is no function to modify it. You can change the current job by
//! [`JobList::set_current_job`].
//!
//! The [`JobList::set_last_async_pid`] function remembers the process ID of the
//! last executed asynchronous command, which will be the value of the `$!`
//! special parameter.

use crate::semantics::ExitStatus;
use crate::trap::Signal;
use nix::sys::wait::WaitStatus;
use slab::Slab;
use std::collections::HashMap;
use std::iter::FusedIterator;
use std::ops::Deref;
use thiserror::Error;

/// Process ID
///
/// A process ID is an integer that identifies a process in the system. This
/// type implements the newtype pattern around the raw integral type `pid_t`
/// declared in the [`libc`] crate. The exact representation of this type
/// depends on the target platform.
///
/// Although genuine process IDs are always positive integers, this type allows
/// zero or negative values for the purpose of specifying a group of processes
/// when used as a parameter to the [`kill`] and [`wait`] system calls. The
/// [`setpgid`] system call also uses process ID zero to specify the process
/// ID of the calling process.
///
/// This type may also be used to represent process group IDs, session IDs, etc.
///
/// [`libc`]: nix::libc
/// [`kill`]: crate::system::System::kill
/// [`wait`]: crate::system::System::wait
/// [`setpgid`]: crate::system::System::setpgid
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pid(pub nix::libc::pid_t);

impl std::fmt::Display for Pid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Neg for Pid {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

/// This conversion depends a type declared in the `nix` crate, which is not
/// covered by the semantic versioning policy of this crate.
impl From<Pid> for nix::unistd::Pid {
    fn from(pid: Pid) -> Self {
        Self::from_raw(pid.0)
    }
}

/// This conversion depends a type declared in the `nix` crate, which is not
/// covered by the semantic versioning policy of this crate.
impl From<nix::unistd::Pid> for Pid {
    fn from(pid: nix::unistd::Pid) -> Self {
        Self(pid.as_raw())
    }
}

impl Pid {
    /// Sentinel value for the [`kill`] and [`wait`]system calls specifying all
    /// processes in the process group of the calling process.
    ///
    /// [`kill`]: crate::system::System::kill
    /// [`wait`]: crate::system::System::wait
    pub const MY_PROCESS_GROUP: Self = Pid(0);

    /// Sentinel value for the [`kill`] and [`wait`] system calls specifying all
    /// possible processes.
    ///
    /// [`kill`]: crate::system::System::kill
    /// [`wait`]: crate::system::System::wait
    pub const ALL: Self = Pid(-1);
}

/// Execution state of a process from which the exit status can be computed
///
/// This type is used to represent the result of a process execution. It is
/// similar to the `WaitStatus` type defined in the `nix` crate, but it is
/// simplified to represent only the states that are relevant to the shell.
///
/// This type only contains the states the process's exit status can be computed
/// from. See also [`ProcessState`], which is a more general type that includes
/// the states that are not directly related to the exit status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessResult {
    /// The process has been stopped by a signal.
    Stopped(Signal),
    /// The process has exited.
    Exited(ExitStatus),
    /// The process has been terminated by a signal.
    Signaled { signal: Signal, core_dump: bool },
}

impl ProcessResult {
    /// Creates a new `ProcessResult` instance representing an exited process.
    #[inline]
    #[must_use]
    pub fn exited<S: Into<ExitStatus>>(exit_status: S) -> Self {
        Self::Exited(exit_status.into())
    }

    /// Whether the process is stopped
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        matches!(self, ProcessResult::Stopped(_))
    }
}

/// Execution state of a process, either running or halted
///
/// This type is used to represent the current state of a process. It is similar
/// to the `WaitStatus` type defined in the `nix` crate, but it is simplified to
/// represent only the states that are relevant to the shell.
///
/// This type can represent all possible states of a process, including running,
/// stopped, exited, and signaled states. When the process is not running, the
/// state is represented by a [`ProcessResult`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    /// The process is running.
    Running,
    /// The process has exited, stopped, or been terminated by a signal.
    Halted(ProcessResult),
}

impl ProcessState {
    /// Creates a new `ProcessState` instance representing a stopped process.
    #[inline]
    #[must_use]
    pub fn stopped(signal: Signal) -> Self {
        Self::Halted(ProcessResult::Stopped(signal))
    }

    /// Creates a new `ProcessState` instance representing an exited process.
    #[inline]
    #[must_use]
    pub fn exited<S: Into<ExitStatus>>(exit_status: S) -> Self {
        Self::Halted(ProcessResult::exited(exit_status))
    }

    /// Whether the process is not yet terminated
    #[must_use]
    pub fn is_alive(&self) -> bool {
        match self {
            _ => todo!(),
            // ProcessState::Running | ProcessState::Stopped(_) => true,
            // ProcessState::Exited(_) | ProcessState::Signaled { .. } => false,
        }
    }

    /// Whether the process is stopped
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        matches!(self, Self::Halted(result) if result.is_stopped())
    }

    /// Converts `ProcessState` to `WaitStatus`.
    ///
    /// This function returns a type defined in the `nix` crate, which is not
    /// covered by the semantic versioning policy of this crate.
    #[must_use]
    pub fn to_wait_status(self, pid: Pid) -> WaitStatus {
        match self {
            _ => todo!(),
            // ProcessState::Running => WaitStatus::Continued(pid.into()),
            // ProcessState::Exited(exit_status) => WaitStatus::Exited(pid.into(), exit_status.0),
            // ProcessState::Stopped(signal) => WaitStatus::Stopped(pid.into(), signal),
            // ProcessState::Signaled { signal, core_dump } => {
            //     WaitStatus::Signaled(pid.into(), signal, core_dump)
            // }
        }
    }

    /// Converts `WaitStatus` to `ProcessState`.
    ///
    /// If the given `WaitStatus` represents a change in the process state, this
    /// function returns the new state with the process ID. Otherwise, it
    /// returns `None`.
    ///
    /// The `WaitStatus` type is defined in the `nix` crate, which is not
    /// covered by the semantic versioning policy of this crate.
    #[must_use]
    pub fn from_wait_status(status: WaitStatus) -> Option<(Pid, Self)> {
        match status {
            WaitStatus::Continued(pid) => Some((pid.into(), ProcessState::Running)),
            // TODO
            // WaitStatus::Exited(pid, exit_status) => {
            //     Some((pid.into(), ProcessState::Exited(ExitStatus(exit_status))))
            // }
            // WaitStatus::Stopped(pid, signal) => Some((pid.into(), ProcessState::Stopped(signal))),
            // WaitStatus::Signaled(pid, signal, core_dump) => {
            //     Some((pid.into(), ProcessState::Signaled { signal, core_dump }))
            // }
            _ => None,
        }
    }
}

impl From<ProcessResult> for ProcessState {
    #[inline]
    fn from(result: ProcessResult) -> Self {
        Self::Halted(result)
    }
}

/// Error value indicating that the process is running.
///
/// This error value may be returned by [`TryFrom<ProcessState>::try_from`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RunningProcess;

/// Converts `ProcessState` to `ExitStatus`.
///
/// For the `Running` state, the conversion fails with [`RunningProcess`].
impl TryFrom<ProcessState> for ExitStatus {
    type Error = RunningProcess;
    fn try_from(state: ProcessState) -> Result<Self, RunningProcess> {
        match state {
            // ProcessState::Exited(exit_status) => Ok(exit_status),
            // ProcessState::Signaled { signal, .. } | ProcessState::Stopped(signal) => {
            //     Ok(ExitStatus::from(signal))
            // }
            ProcessState::Running => Err(RunningProcess),
            _ => todo!(),
        }
    }
}

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
    ///
    /// If the job is job-controlled, this is also the process group ID.
    pub pid: Pid,

    /// Whether the job is job-controlled.
    ///
    /// If the job is job-controlled, the job processes run in their own process
    /// group.
    pub job_controlled: bool,

    /// Current state of the process
    pub state: ProcessState,

    /// State of the process expected in the next update
    ///
    /// See [`JobRefMut::expect`] and [`JobList::update_status`] for details.
    pub expected_state: Option<ProcessState>,

    /// Indicator of state change
    ///
    /// This flag is true if the `state` has been changed since the state was
    /// last reported to the user.
    pub state_changed: bool,

    /// Whether this job is a true child of the current shell
    ///
    /// When a subshell is created, the jobs inherited from the parent shell are
    /// marked as not owned by the current shell. The shell cannot wait for
    /// these jobs to finish.
    pub is_owned: bool,

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
            state: ProcessState::Running,
            expected_state: None,
            state_changed: true,
            is_owned: true,
            name: String::new(),
        }
    }

    fn is_suspended(&self) -> bool {
        todo!() // matches!(self.state, ProcessState::Stopped(_))
    }
}

/// Partially mutable reference to [`Job`].
///
/// This struct is a specialized reference type for `Job`. It provides limited
/// mutability over the `Job` instance through its methods. It also allows
/// unlimited immutable access through the `Deref` implementation.
#[derive(Debug, Eq, PartialEq)]
pub struct JobRefMut<'a>(&'a mut Job);

impl JobRefMut<'_> {
    /// Sets the `expected_state` of the job.
    ///
    /// This method remembers the argument as the expected state of the job.
    /// If the job is [updated] with the same state, the `state_changed` flag
    /// is not set then.
    ///
    /// This method may be used to suppress a change report of a job state,
    /// especially when the state is reported before it is actually changed.
    ///
    /// [updated]: JobList::update_status
    pub fn expect<S>(&mut self, state: S)
    where
        S: Into<Option<ProcessState>>,
    {
        self.0.expected_state = state.into();
    }

    /// Clears the `state_changed` flag of the job.
    ///
    /// Normally, this method should be called when the shell printed a job
    /// status report.
    pub fn state_reported(&mut self) {
        self.0.state_changed = false
    }
}

impl Deref for JobRefMut<'_> {
    type Target = Job;
    fn deref(&self) -> &Job {
        self.0
    }
}

/// Indexed iterator of jobs.
///
/// Call [`JobList::iter`] to get an instance of `Iter`.
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

/// Indexed iterator of partially mutable jobs.
///
/// Call [`JobList::iter_mut`] to get an instance of `IterMut`.
#[derive(Debug)]
pub struct IterMut<'a>(slab::IterMut<'a, Job>);

impl<'a> Iterator for IterMut<'a> {
    type Item = (usize, JobRefMut<'a>);

    #[inline]
    fn next(&mut self) -> Option<(usize, JobRefMut<'a>)> {
        self.0.next().map(|(index, job)| (index, JobRefMut(job)))
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a> DoubleEndedIterator for IterMut<'a> {
    fn next_back(&mut self) -> Option<(usize, JobRefMut<'a>)> {
        self.0
            .next_back()
            .map(|(index, job)| (index, JobRefMut(job)))
    }
}

impl ExactSizeIterator for IterMut<'_> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl FusedIterator for IterMut<'_> {}

/// Collection of jobs.
///
/// See the [module documentation](self) for details.
#[derive(Clone, Debug)]
pub struct JobList {
    /// Jobs managed by the shell
    jobs: Slab<Job>,

    /// Map from process IDs to indices of `jobs`
    ///
    /// This is a shortcut to quickly find jobs by process ID.
    pids_to_indices: HashMap<Pid, usize>,

    /// Index of the current job. (Only valid when the list is non-empty)
    current_job_index: usize,

    /// Index of the previous job. (Only valid when the list is non-empty)
    previous_job_index: usize,

    /// Process ID of the most recently executed asynchronous command.
    last_async_pid: Pid,
}

impl Default for JobList {
    fn default() -> Self {
        JobList {
            jobs: Slab::new(),
            pids_to_indices: HashMap::new(),
            current_job_index: usize::default(),
            previous_job_index: usize::default(),
            last_async_pid: Pid(0),
        }
    }
}

impl JobList {
    /// Creates an empty job list.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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

    /// Returns the number of jobs in this job list.
    #[inline]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Returns true if this job list contains no jobs.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an indexed iterator of jobs.
    ///
    /// The item type of the returned iterator is `(usize, &Job)`.
    /// Jobs are iterated in the order of indices.
    #[inline]
    pub fn iter(&self) -> Iter {
        Iter(self.jobs.iter())
    }

    /// Returns an indexed iterator of partially mutable jobs.
    ///
    /// The item type of the returned iterator is `(usize, JobRefMut)`.
    /// Note that the iterator does not yield raw mutable references to jobs.
    /// [`JobRefMut`] allows mutating only part of jobs.
    ///
    /// Jobs are iterated in the order of indices.
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut {
        IterMut(self.jobs.iter_mut())
    }

    /// Finds a job by the process ID.
    ///
    /// This function returns the index of the job whose process ID is `pid`.
    /// The result is `None` if no such job is found.
    ///
    /// A `JobList` maintains an internal hash map to quickly find jobs by
    /// process ID.
    pub fn find_by_pid(&self, pid: Pid) -> Option<usize> {
        self.pids_to_indices.get(&pid).copied()
    }
}

impl<'a> IntoIterator for &'a JobList {
    type Item = (usize, &'a Job);
    type IntoIter = Iter<'a>;
    #[inline(always)]
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut JobList {
    type Item = (usize, JobRefMut<'a>);
    type IntoIter = IterMut<'a>;
    #[inline(always)]
    fn into_iter(self) -> IterMut<'a> {
        self.iter_mut()
    }
}

/// Supports indexing operation on `JobList`.
impl std::ops::Index<usize> for JobList {
    type Output = Job;

    /// Returns a reference to the specified job.
    ///
    /// This function will panic if the job does not exist.
    fn index(&self, index: usize) -> &Job {
        &self.jobs[index]
    }
}

/// Iterator that conditionally removes jobs from a job list.
///
/// Call [`JobList::extract_if`] to get an instance of `ExtractIf`.
#[derive(Debug)]
pub struct ExtractIf<'a, F>
where
    F: FnMut(usize, JobRefMut) -> bool,
{
    list: &'a mut JobList,
    should_remove: F,
    next_index: usize,
    len: usize,
}

impl<F> Iterator for ExtractIf<'_, F>
where
    F: FnMut(usize, JobRefMut) -> bool,
{
    type Item = (usize, Job);

    fn next(&mut self) -> Option<(usize, Job)> {
        while self.len > 0 {
            let index = self.next_index;
            self.next_index += 1;
            if let Some(job) = self.list.get_mut(index) {
                self.len -= 1;
                if (self.should_remove)(index, job) {
                    let job = self.list.remove(index).unwrap();
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

impl<F> FusedIterator for ExtractIf<'_, F> where F: FnMut(usize, JobRefMut) -> bool {}

impl JobList {
    /// Adds a job to this job list.
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
        let ex_current_job_is_suspended =
            self.current_job().map(|index| self[index].is_suspended());
        let ex_previous_job_is_suspended =
            self.previous_job().map(|index| self[index].is_suspended());

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

    /// Removes a job from this job list.
    ///
    /// This function returns the job removed from the job list.
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

    /// Removes jobs that satisfy the predicate.
    ///
    /// This function uses the `should_remove` function to decide whether to
    /// remove jobs. If it returns true, the job is removed and yielded from the
    /// iterator. Otherwise, the job remains in the list.
    ///
    /// You can reset the `state_changed` flag of a job
    /// ([`JobRefMut::state_reported`]) regardless of whether you choose to
    /// remove it or not.
    ///
    /// This function is a simplified version of [`JobList::extract_if`] that
    /// does not return removed jobs.
    pub fn remove_if<F>(&mut self, should_remove: F)
    where
        F: FnMut(usize, JobRefMut) -> bool,
    {
        self.extract_if(should_remove).for_each(drop)
    }

    /// Returns an iterator that conditionally removes jobs.
    ///
    /// The iterator uses the `should_remove` function to decide whether to
    /// remove jobs. If it returns true, the job is removed and yielded from the
    /// iterator. Otherwise, the job remains in the list.
    ///
    /// You can reset the `state_changed` flag of a job
    /// ([`JobRefMut::state_reported`]) regardless of whether you choose to
    /// remove it or not.
    ///
    /// If the returned iterator is dropped before iterating all jobs, the
    /// remaining jobs are retained in the list.
    ///
    /// If you don't need to take the ownership of removed jobs, consider using
    /// [`JobList::remove_if`] instead.
    pub fn extract_if<F>(&mut self, should_remove: F) -> ExtractIf<'_, F>
    where
        F: FnMut(usize, JobRefMut) -> bool,
    {
        let len = self.len();
        ExtractIf {
            list: self,
            should_remove,
            next_index: 0,
            len,
        }
    }
}

impl JobList {
    /// Updates the state of a job.
    ///
    /// The result of a [`wait`](crate::System::wait) call should be passed to
    /// this function. It looks up the job for the given process ID, updates the
    /// state of the job to the given `state`, and sets the `state_changed` flag
    /// in the job. As an exception, if `state` is equal to the `expected_state`
    /// of the job, the `state_changed` flag is not set. The `expected_state` is
    /// cleared in any case. (See also [`JobRefMut::expect`] for the usage of
    /// `expected_state`.)
    ///
    /// Returns the index of the job updated. If there is no job for the given
    /// process ID, the result is `None`.
    ///
    /// When a job is suspended (i.e., `state` is `Stopped`), the job becomes
    /// the [current job](Self::current_job) and the old current job becomes the
    /// [previous job](Self::previous_job). When a suspended job gets a state
    /// update:
    ///
    /// - If the updated job is the current job and the previous job is
    ///   suspended, the previous job becomes the current job and the new
    ///   previous job is chosen from other suspended jobs. If there is no
    ///   suspended jobs, the new previous jobs is the old current job.
    /// - If the updated job is the previous job and there is a suspended job
    ///   other than the current job, it becomes the previous job.
    pub fn update_status(&mut self, pid: Pid, state: ProcessState) -> Option<usize> {
        let index = self.find_by_pid(pid)?;

        // Update the job state.
        let job = &mut self.jobs[index];
        let was_suspended = job.is_suspended();
        job.state = state;
        job.state_changed |= job.expected_state != Some(state);
        job.expected_state = None;

        // Reselect the current and previous job.
        if !was_suspended && job.is_suspended() {
            if index != self.current_job_index {
                self.previous_job_index = std::mem::replace(&mut self.current_job_index, index);
            }
        } else if was_suspended && !job.is_suspended() {
            if let Some(prev_index) = self.previous_job() {
                let previous_job_becomes_current_job =
                    index == self.current_job_index && self[prev_index].is_suspended();
                if previous_job_becomes_current_job {
                    self.current_job_index = prev_index;
                }
                if previous_job_becomes_current_job || index == prev_index {
                    self.previous_job_index = self.any_suspended_job_but_current().unwrap_or(index);
                }
            }
        }

        Some(index)
    }

    /// Disowns all jobs.
    ///
    /// This function sets the `is_owned` flag of all jobs to `false`.
    pub fn disown_all(&mut self) {
        for (_, job) in &mut self.jobs {
            job.is_owned = false;
        }
    }
}

/// Error type for [`JobList::set_current_job`].
#[derive(Clone, Copy, Debug, Eq, Error, Hash, PartialEq)]
pub enum SetCurrentJobError {
    /// The specified index does not refer to any job.
    #[error("no such job")]
    NoSuchJob,

    /// The specified job is not a suspended job and there is another suspended
    /// job.
    #[error("the current job must be selected from suspended jobs")]
    NotSuspended,
}

impl JobList {
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

    /// Returns the index of the current job.
    ///
    /// If the job list contains at least one job, there is a current job. This
    /// function returns its index. If the job list is empty, the result is
    /// `None`.
    ///
    /// If there is any suspended jobs, the current job must be a suspended job.
    /// Running or terminated jobs can be the current job if there is no
    /// suspended job. You can [change the current job](Self::set_current_job)
    /// as long as the above rules are met.
    ///
    /// See also [`previous_job`](Self::previous_job).
    pub fn current_job(&self) -> Option<usize> {
        if self.jobs.contains(self.current_job_index) {
            Some(self.current_job_index)
        } else {
            None
        }
    }

    /// Returns the index of the previous job.
    ///
    /// If the job list contains two or more jobs, there is a previous job. This
    /// function returns its index. If the job list has zero or one job, the
    /// result is `None`.
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
    pub fn previous_job(&self) -> Option<usize> {
        if self.previous_job_index != self.current_job_index
            && self.jobs.contains(self.previous_job_index)
        {
            Some(self.previous_job_index)
        } else {
            None
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

impl JobList {
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

pub mod fmt;
pub mod id;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trap::Signal;

    #[test]
    fn job_list_find_by_pid() {
        let mut list = JobList::default();
        assert_eq!(list.find_by_pid(Pid(10)), None);

        let i10 = list.add(Job::new(Pid(10)));
        let i20 = list.add(Job::new(Pid(20)));
        let i30 = list.add(Job::new(Pid(30)));
        assert_eq!(list.find_by_pid(Pid(10)), Some(i10));
        assert_eq!(list.find_by_pid(Pid(20)), Some(i20));
        assert_eq!(list.find_by_pid(Pid(30)), Some(i30));
        assert_eq!(list.find_by_pid(Pid(40)), None);

        list.remove(i10);
        assert_eq!(list.find_by_pid(Pid(10)), None);
    }

    #[test]
    fn job_list_add_and_remove() {
        // This test case depends on how Slab reuses the index of removed items.
        let mut list = JobList::default();

        assert_eq!(list.add(Job::new(Pid(10))), 0);
        assert_eq!(list.add(Job::new(Pid(11))), 1);
        assert_eq!(list.add(Job::new(Pid(12))), 2);

        assert_eq!(list.remove(0).unwrap().pid, Pid(10));
        assert_eq!(list.remove(1).unwrap().pid, Pid(11));

        // Indices are reused in the reverse order of removals.
        assert_eq!(list.add(Job::new(Pid(13))), 1);
        assert_eq!(list.add(Job::new(Pid(14))), 0);

        assert_eq!(list.remove(0).unwrap().pid, Pid(14));
        assert_eq!(list.remove(1).unwrap().pid, Pid(13));
        assert_eq!(list.remove(2).unwrap().pid, Pid(12));

        // Once the job list is empty, indices start from 0 again.
        assert_eq!(list.add(Job::new(Pid(13))), 0);
        assert_eq!(list.add(Job::new(Pid(14))), 1);
    }

    #[test]
    fn job_list_add_same_pid() {
        let mut list = JobList::default();

        let mut job = Job::new(Pid(10));
        job.name = "first job".to_string();
        let i_first = list.add(job);

        let mut job = Job::new(Pid(10));
        job.name = "second job".to_string();
        let i_second = list.add(job);

        let job = &list[i_second];
        assert_eq!(job.pid, Pid(10));
        assert_eq!(job.name, "second job");

        assert_ne!(
            list.get(i_first).map(|job| job.name.as_str()),
            Some("first job")
        );
    }

    #[test]
    fn job_list_extract_if() {
        let mut list = JobList::default();
        let i21 = list.add(Job::new(Pid(21)));
        let i22 = list.add(Job::new(Pid(22)));
        let i23 = list.add(Job::new(Pid(23)));
        let i24 = list.add(Job::new(Pid(24)));
        let i25 = list.add(Job::new(Pid(25)));
        let i26 = list.add(Job::new(Pid(26)));
        list.remove(i23).unwrap();

        let mut i = list.extract_if(|index, mut job| {
            assert_ne!(index, i23);
            if index % 2 == 0 {
                job.state_reported();
            }
            index == 0 || job.pid == Pid(26)
        });

        let mut expected_job_21 = Job::new(Pid(21));
        expected_job_21.state_changed = false;
        assert_eq!(i.next(), Some((i21, expected_job_21)));
        assert_eq!(i.next(), Some((i26, Job::new(Pid(26)))));
        assert_eq!(i.next(), None);
        assert_eq!(i.next(), None); // ExtractIf is fused.

        let indices: Vec<usize> = list.iter().map(|(index, _)| index).collect();
        assert_eq!(indices, [i22, i24, i25]);
        assert!(list[i22].state_changed);
        assert!(list[i24].state_changed);
        assert!(!list[i25].state_changed);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn updating_job_status_without_expected_state() {
        let mut list = JobList::default();
        let state = todo!(); //ProcessState::Exited(ExitStatus(15));
        assert_eq!(list.update_status(Pid(20), state), None);

        let i10 = list.add(Job::new(Pid(10)));
        let i20 = list.add(Job::new(Pid(20)));
        let i30 = list.add(Job::new(Pid(30)));
        assert_eq!(list[i20].state, ProcessState::Running);

        list.get_mut(i20).unwrap().state_reported();
        assert_eq!(list[i20].state_changed, false);

        assert_eq!(list.update_status(Pid(20), state), Some(i20));
        // TODO assert_eq!(list[i20].state, ProcessState::Exited(ExitStatus(15)));
        assert_eq!(list[i20].state_changed, true);

        assert_eq!(list[i10].state, ProcessState::Running);
        assert_eq!(list[i30].state, ProcessState::Running);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn updating_job_status_with_matching_expected_state() {
        let mut list = JobList::default();
        let pid = Pid(20);
        let mut job = Job::new(pid);
        job.expected_state = Some(ProcessState::Running);
        job.state_changed = false;
        let i20 = list.add(job);

        assert_eq!(list.update_status(pid, ProcessState::Running), Some(i20));

        let job = &list[i20];
        assert_eq!(job.state, ProcessState::Running);
        assert_eq!(job.expected_state, None);
        assert_eq!(job.state_changed, false);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn updating_job_status_with_unmatched_expected_state() {
        let mut list = JobList::default();
        let pid = Pid(20);
        let mut job = Job::new(pid);
        job.expected_state = Some(ProcessState::Running);
        job.state_changed = false;
        let i20 = list.add(job);

        let result = todo!(); // list.update_status(pid, ProcessState::Exited(ExitStatus(0)));
        // TODO assert_eq!(result, Some(i20));

        let job = &list[i20];
        // TODO assert_eq!(job.state, ProcessState::Exited(ExitStatus(0)));
        assert_eq!(job.expected_state, None);
        assert_eq!(job.state_changed, true);
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn disowning_jobs() {
        let mut list = JobList::default();
        let i10 = list.add(Job::new(Pid(10)));
        let i20 = list.add(Job::new(Pid(20)));
        let i30 = list.add(Job::new(Pid(30)));

        list.disown_all();

        assert_eq!(list[i10].is_owned, false);
        assert_eq!(list[i20].is_owned, false);
        assert_eq!(list[i30].is_owned, false);
    }

    #[test]
    fn no_current_and_previous_job_in_empty_job_list() {
        let list = JobList::default();
        assert_eq!(list.current_job(), None);
        assert_eq!(list.previous_job(), None);
    }

    #[test]
    fn current_and_previous_job_in_job_list_with_one_job() {
        let mut list = JobList::default();
        let i10 = list.add(Job::new(Pid(10)));
        assert_eq!(list.current_job(), Some(i10));
        assert_eq!(list.previous_job(), None);
    }

    #[test]
    fn current_and_previous_job_in_job_list_with_two_job() {
        // If one job is suspended and the other is not, the current job is the
        // suspended one.
        let mut list = JobList::default();
        let mut suspended = Job::new(Pid(10));
        suspended.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        let running = Job::new(Pid(20));
        let i10 = list.add(suspended.clone());
        let i20 = list.add(running.clone());
        assert_eq!(list.current_job(), Some(i10));
        assert_eq!(list.previous_job(), Some(i20));

        // The order of adding jobs does not matter in this case.
        list = JobList::default();
        let i20 = list.add(running);
        let i10 = list.add(suspended);
        assert_eq!(list.current_job(), Some(i10));
        assert_eq!(list.previous_job(), Some(i20));
    }

    #[test]
    fn adding_suspended_job_with_running_current_and_previous_job() {
        let mut list = JobList::default();
        let running_1 = Job::new(Pid(11));
        let running_2 = Job::new(Pid(12));
        list.add(running_1);
        list.add(running_2);
        let ex_current_job_index = list.current_job().unwrap();
        let ex_previous_job_index = list.previous_job().unwrap();
        assert_ne!(ex_current_job_index, ex_previous_job_index);

        let mut suspended = Job::new(Pid(20));
        suspended.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        let i20 = list.add(suspended);
        let now_current_job_index = list.current_job().unwrap();
        let now_previous_job_index = list.previous_job().unwrap();
        assert_eq!(now_current_job_index, i20);
        assert_eq!(now_previous_job_index, ex_current_job_index);
    }

    #[test]
    fn adding_suspended_job_with_suspended_current_and_running_previous_job() {
        let mut list = JobList::default();

        let running = Job::new(Pid(18));
        let i18 = list.add(running);

        let mut suspended_1 = Job::new(Pid(19));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        let i19 = list.add(suspended_1);

        let ex_current_job_index = list.current_job().unwrap();
        let ex_previous_job_index = list.previous_job().unwrap();
        assert_eq!(ex_current_job_index, i19);
        assert_eq!(ex_previous_job_index, i18);

        let mut suspended_2 = Job::new(Pid(20));
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        let i20 = list.add(suspended_2);

        let now_current_job_index = list.current_job().unwrap();
        let now_previous_job_index = list.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_current_job_index);
        assert_eq!(now_previous_job_index, i20);
    }

    #[test]
    fn removing_current_job() {
        let mut list = JobList::default();

        let running = Job::new(Pid(10));
        let i10 = list.add(running);

        let mut suspended_1 = Job::new(Pid(11));
        let mut suspended_2 = Job::new(Pid(12));
        let mut suspended_3 = Job::new(Pid(13));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        suspended_3.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        list.add(suspended_1);
        list.add(suspended_2);
        list.add(suspended_3);

        let current_job_index_1 = list.current_job().unwrap();
        let previous_job_index_1 = list.previous_job().unwrap();
        assert_ne!(current_job_index_1, i10);
        assert_ne!(previous_job_index_1, i10);

        list.remove(current_job_index_1);
        let current_job_index_2 = list.current_job().unwrap();
        let previous_job_index_2 = list.previous_job().unwrap();
        assert_eq!(current_job_index_2, previous_job_index_1);
        assert_ne!(previous_job_index_2, current_job_index_2);
        // The new previous job is chosen from suspended jobs other than the current job.
        let previous_job_2 = &list[previous_job_index_2];
        assert!(
            previous_job_2.is_suspended(),
            "previous_job_2 = {previous_job_2:?}"
        );

        list.remove(current_job_index_2);
        let current_job_index_3 = list.current_job().unwrap();
        let previous_job_index_3 = list.previous_job().unwrap();
        assert_eq!(current_job_index_3, previous_job_index_2);
        // There is no other suspended job, so the new previous job is a running job.
        assert_eq!(previous_job_index_3, i10);

        list.remove(current_job_index_3);
        let current_job_index_4 = list.current_job().unwrap();
        assert_eq!(current_job_index_4, i10);
        // No more job to be selected for the previous job.
        assert_eq!(list.previous_job(), None);
    }

    #[test]
    fn removing_previous_job_with_suspended_job() {
        let mut list = JobList::default();

        let running = Job::new(Pid(10));
        let i10 = list.add(running);

        let mut suspended_1 = Job::new(Pid(11));
        let mut suspended_2 = Job::new(Pid(12));
        let mut suspended_3 = Job::new(Pid(13));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        suspended_3.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        list.add(suspended_1);
        list.add(suspended_2);
        list.add(suspended_3);

        let ex_current_job_index = list.current_job().unwrap();
        let ex_previous_job_index = list.previous_job().unwrap();
        assert_ne!(ex_current_job_index, i10);
        assert_ne!(ex_previous_job_index, i10);

        list.remove(ex_previous_job_index);
        let now_current_job_index = list.current_job().unwrap();
        let now_previous_job_index = list.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_current_job_index);
        assert_ne!(now_previous_job_index, now_current_job_index);
        // The new previous job is chosen from suspended jobs other than the current job.
        let now_previous_job = &list[now_previous_job_index];
        assert!(
            now_previous_job.is_suspended(),
            "now_previous_job = {now_previous_job:?}"
        );
    }

    #[test]
    fn removing_previous_job_with_running_job() {
        let mut list = JobList::default();

        let running = Job::new(Pid(10));
        let i10 = list.add(running);

        let mut suspended_1 = Job::new(Pid(11));
        let mut suspended_2 = Job::new(Pid(12));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        list.add(suspended_1);
        list.add(suspended_2);

        let ex_current_job_index = list.current_job().unwrap();
        let ex_previous_job_index = list.previous_job().unwrap();
        assert_ne!(ex_current_job_index, i10);
        assert_ne!(ex_previous_job_index, i10);

        list.remove(ex_previous_job_index);
        let now_current_job_index = list.current_job().unwrap();
        let now_previous_job_index = list.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_current_job_index);
        // When there is no suspended job other than the current job,
        // then the new previous job can be any job other than the current.
        assert_eq!(now_previous_job_index, i10);
    }

    #[test]
    fn set_current_job_with_running_jobs_only() {
        let mut list = JobList::default();
        let i21 = list.add(Job::new(Pid(21)));
        let i22 = list.add(Job::new(Pid(22)));

        assert_eq!(list.set_current_job(i21), Ok(()));
        assert_eq!(list.current_job(), Some(i21));

        assert_eq!(list.set_current_job(i22), Ok(()));
        assert_eq!(list.current_job(), Some(i22));
    }

    #[test]
    fn set_current_job_to_suspended_job() {
        let mut list = JobList::default();
        list.add(Job::new(Pid(20)));

        let mut suspended_1 = Job::new(Pid(21));
        let mut suspended_2 = Job::new(Pid(22));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGSTOP);
        let i21 = list.add(suspended_1);
        let i22 = list.add(suspended_2);

        assert_eq!(list.set_current_job(i21), Ok(()));
        assert_eq!(list.current_job(), Some(i21));

        assert_eq!(list.set_current_job(i22), Ok(()));
        assert_eq!(list.current_job(), Some(i22));
    }

    #[test]
    fn set_current_job_no_such_job() {
        let mut list = JobList::default();
        assert_eq!(list.set_current_job(0), Err(SetCurrentJobError::NoSuchJob));
        assert_eq!(list.set_current_job(1), Err(SetCurrentJobError::NoSuchJob));
        assert_eq!(list.set_current_job(2), Err(SetCurrentJobError::NoSuchJob));
    }

    #[test]
    fn set_current_job_not_suspended() {
        let mut list = JobList::default();
        let mut suspended = Job::new(Pid(10));
        suspended.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        let running = Job::new(Pid(20));
        let i10 = list.add(suspended);
        let i20 = list.add(running);
        assert_eq!(
            list.set_current_job(i20),
            Err(SetCurrentJobError::NotSuspended)
        );
        assert_eq!(list.current_job(), Some(i10));
    }

    #[test]
    fn set_current_job_no_change() {
        let mut list = JobList::default();
        list.add(Job::new(Pid(5)));
        list.add(Job::new(Pid(6)));
        let old_current_job_index = list.current_job().unwrap();
        let old_previous_job_index = list.previous_job().unwrap();
        list.set_current_job(old_current_job_index).unwrap();
        let new_current_job_index = list.current_job().unwrap();
        let new_previous_job_index = list.previous_job().unwrap();
        assert_eq!(new_current_job_index, old_current_job_index);
        assert_eq!(new_previous_job_index, old_previous_job_index);
    }

    #[test]
    fn resuming_current_job_without_other_suspended_jobs() {
        let mut list = JobList::default();
        let mut suspended = Job::new(Pid(10));
        suspended.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        let running = Job::new(Pid(20));
        let i10 = list.add(suspended);
        let i20 = list.add(running);
        list.update_status(Pid(10), ProcessState::Running);
        assert_eq!(list.current_job(), Some(i10));
        assert_eq!(list.previous_job(), Some(i20));
    }

    #[test]
    fn resuming_current_job_with_another_suspended_job() {
        let mut list = JobList::default();
        let mut suspended_1 = Job::new(Pid(10));
        let mut suspended_2 = Job::new(Pid(20));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        let i10 = list.add(suspended_1);
        let i20 = list.add(suspended_2);
        list.set_current_job(i10).unwrap();
        list.update_status(Pid(10), ProcessState::Running);
        // The current job must be a suspended job, if any.
        assert_eq!(list.current_job(), Some(i20));
        assert_eq!(list.previous_job(), Some(i10));
    }

    #[test]
    fn resuming_current_job_with_other_suspended_jobs() {
        let mut list = JobList::default();
        let mut suspended_1 = Job::new(Pid(10));
        let mut suspended_2 = Job::new(Pid(20));
        let mut suspended_3 = Job::new(Pid(30));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_3.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        list.add(suspended_1);
        list.add(suspended_2);
        list.add(suspended_3);
        let ex_current_job_pid = list[list.current_job().unwrap()].pid;
        let ex_previous_job_index = list.previous_job().unwrap();

        list.update_status(ex_current_job_pid, ProcessState::Running);
        let now_current_job_index = list.current_job().unwrap();
        let now_previous_job_index = list.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_previous_job_index);
        assert_ne!(now_previous_job_index, now_current_job_index);
        // The new previous job is chosen from suspended jobs other than the current job.
        let now_previous_job = &list[now_previous_job_index];
        assert!(
            now_previous_job.is_suspended(),
            "now_previous_job = {now_previous_job:?}"
        );
    }

    #[test]
    fn resuming_previous_job() {
        let mut list = JobList::default();
        let mut suspended_1 = Job::new(Pid(10));
        let mut suspended_2 = Job::new(Pid(20));
        let mut suspended_3 = Job::new(Pid(30));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_3.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        list.add(suspended_1);
        list.add(suspended_2);
        list.add(suspended_3);
        let ex_current_job_index = list.current_job().unwrap();
        let ex_previous_job_pid = list[list.previous_job().unwrap()].pid;

        list.update_status(ex_previous_job_pid, ProcessState::Running);
        let now_current_job_index = list.current_job().unwrap();
        let now_previous_job_index = list.previous_job().unwrap();
        assert_eq!(now_current_job_index, ex_current_job_index);
        assert_ne!(now_previous_job_index, now_current_job_index);
        // The new previous job is chosen from suspended jobs other than the current job.
        let now_previous_job = &list[now_previous_job_index];
        assert!(
            now_previous_job.is_suspended(),
            "now_previous_job = {now_previous_job:?}"
        );
    }

    #[test]
    fn resuming_other_job() {
        let mut list = JobList::default();
        let mut suspended_1 = Job::new(Pid(10));
        let mut suspended_2 = Job::new(Pid(20));
        let mut suspended_3 = Job::new(Pid(30));
        suspended_1.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_2.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        suspended_3.state = todo!(); // ProcessState::Stopped(Signal::SIGTSTP);
        let i10 = list.add(suspended_1);
        let i20 = list.add(suspended_2);
        let _i30 = list.add(suspended_3);
        list.set_current_job(i20).unwrap();
        list.set_current_job(i10).unwrap();
        list.update_status(Pid(30), ProcessState::Running);
        assert_eq!(list.current_job(), Some(i10));
        assert_eq!(list.previous_job(), Some(i20));
    }

    #[test]
    fn suspending_current_job() {
        let mut list = JobList::default();
        let i11 = list.add(Job::new(Pid(11)));
        let i12 = list.add(Job::new(Pid(12)));
        list.set_current_job(i11).unwrap();
        // TODO list.update_status(Pid(11), ProcessState::Stopped(Signal::SIGTTOU));
        assert_eq!(list.current_job(), Some(i11));
        assert_eq!(list.previous_job(), Some(i12));
    }

    #[test]
    fn suspending_previous_job() {
        let mut list = JobList::default();
        let i11 = list.add(Job::new(Pid(11)));
        let i12 = list.add(Job::new(Pid(12)));
        list.set_current_job(i11).unwrap();
        // TODO list.update_status(Pid(12), ProcessState::Stopped(Signal::SIGTTOU));
        assert_eq!(list.current_job(), Some(i12));
        assert_eq!(list.previous_job(), Some(i11));
    }

    #[test]
    fn suspending_job_with_running_current_job() {
        let mut list = JobList::default();
        let i10 = list.add(Job::new(Pid(10)));
        let _i11 = list.add(Job::new(Pid(11)));
        let i12 = list.add(Job::new(Pid(12)));
        list.set_current_job(i10).unwrap();
        // TODO list.update_status(Pid(12), ProcessState::Stopped(Signal::SIGTTIN));
        assert_eq!(list.current_job(), Some(i12));
        assert_eq!(list.previous_job(), Some(i10));
    }

    #[test]
    fn suspending_job_with_running_previous_job() {
        let mut list = JobList::default();
        let i11 = list.add(Job::new(Pid(11)));
        let i12 = list.add(Job::new(Pid(12)));
        let mut suspended = Job::new(Pid(10));
        suspended.state = todo!(); // ProcessState::Stopped(Signal::SIGTTIN);
        let i10 = list.add(suspended);
        assert_eq!(list.current_job(), Some(i10));
        assert_eq!(list.previous_job(), Some(i11));

        // TODO list.update_status(Pid(12), ProcessState::Stopped(Signal::SIGTTOU));
        assert_eq!(list.current_job(), Some(i12));
        assert_eq!(list.previous_job(), Some(i10));
    }
}
