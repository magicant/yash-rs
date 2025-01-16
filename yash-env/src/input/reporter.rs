// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

use crate::job::fmt::Accumulator;
use crate::option::{Interactive, Monitor, Off};
use crate::Env;
use std::cell::RefCell;
use yash_syntax::input::{Context, Input, Result};
use yash_syntax::syntax::Fd;

/// `Input` decorator that reports job status changes before reading a line
///
/// This decorator prints the status of jobs that have changed since the last
/// report. The status is printed to the standard error before the input is read.
/// This is done only if the [`Interactive`] and [`Monitor`] options are enabled.
#[derive(Clone, Debug)]
pub struct Reporter<'a, 'b, T> {
    inner: T,
    env: &'a RefCell<&'b mut Env>,
}

impl<'a, 'b, T> Reporter<'a, 'b, T> {
    /// Creates a new `Reporter` decorator.
    ///
    /// The first argument is the inner `Input` that performs the actual input
    /// operation. The second argument is the shell environment that contains
    /// the shell option state and the system interface to print to the standard
    /// error. It is wrapped in a `RefCell` so that it can be shared with other
    /// decorators and the parser.
    pub fn new(inner: T, env: &'a RefCell<&'b mut Env>) -> Self {
        Self { inner, env }
    }
}

impl<T> Input for Reporter<'_, '_, T>
where
    T: Input,
{
    #[allow(clippy::await_holding_refcell_ref)]
    async fn next_line(&mut self, context: &Context) -> Result {
        report(&mut self.env.borrow_mut()).await;
        self.inner.next_line(context).await
    }
}

async fn report(env: &mut Env) {
    if env.options.get(Interactive) == Off || env.options.get(Monitor) == Off {
        return;
    }

    let mut accumulator = Accumulator::new();
    accumulator.current_job_index = env.jobs.current_job();
    accumulator.previous_job_index = env.jobs.previous_job();
    env.jobs
        .iter()
        .filter(|(_, job)| job.state_changed)
        .for_each(|(index, job)| accumulator.add(index, job, &env.system));

    if env
        .system
        .write_all(Fd::STDERR, accumulator.print.as_bytes())
        .await
        .is_ok()
    {
        for index in accumulator.indices_reported {
            env.jobs.get_mut(index).unwrap().state_reported();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{Job, Pid, ProcessState};
    use crate::option::On;
    use crate::system::r#virtual::SystemState;
    use crate::tests::assert_stderr;
    use crate::VirtualSystem;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_syntax::input::Memory;

    #[test]
    fn reporter_reads_from_inner_input() {
        let mut env = Env::new_virtual();
        let ref_env = RefCell::new(&mut env);
        let mut reporter = Reporter::new(Memory::new("echo hello"), &ref_env);
        let result = reporter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo hello");
    }

    #[test]
    fn reporter_shows_job_status_before_reading_input() {
        let system = Box::new(VirtualSystem::new());
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.jobs.add({
            let mut job = Job::new(Pid(10));
            job.state_changed = true;
            job.name = "echo hello".to_string();
            job
        });
        env.options.set(Interactive, On);
        env.options.set(Monitor, On);

        struct InputMock(Rc<RefCell<SystemState>>);
        impl Input for InputMock {
            async fn next_line(&mut self, _: &Context) -> Result {
                // The Report is expected to have shown the report before
                // calling the inner input. Let's check that here.
                assert_stderr(&self.0, |stderr| {
                    assert!(stderr.starts_with("[1]"), "stderr: {stderr:?}")
                });

                Ok("foo".to_string())
            }
        }

        let ref_env = RefCell::new(&mut env);
        let mut reporter = Reporter::new(InputMock(state), &ref_env);
        let result = reporter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "foo"); // Make sure the mock input is called.
    }

    #[test]
    fn all_jobs_with_changed_status_are_reported() {
        let system = Box::new(VirtualSystem::new());
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.jobs.add({
            let mut job = Job::new(Pid(10));
            job.state_changed = true;
            job.name = "echo hello".to_string();
            job
        });
        env.jobs.add({
            let mut job = Job::new(Pid(20));
            job.state_changed = false;
            job.name = "sleep 1".to_string();
            job
        });
        env.jobs.add({
            let mut job = Job::new(Pid(30));
            job.state = ProcessState::exited(0);
            job.state_changed = true;
            job.name = "cat README".to_string();
            job
        });
        env.options.set(Interactive, On);
        env.options.set(Monitor, On);
        let ref_env = RefCell::new(&mut env);
        let memory = Memory::new("echo hello\n");
        let mut reporter = Reporter::new(memory, &ref_env);

        reporter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_stderr(&state, |stderr| {
            let mut lines = stderr.lines();
            let first = lines.next().unwrap();
            assert!(first.starts_with("[1]"), "first: {first:?}");
            assert!(first.contains("Running"), "first: {first:?}");
            assert!(first.contains("echo hello"), "first: {first:?}");
            let second = lines.next().unwrap();
            assert!(second.starts_with("[3]"), "second: {second:?}");
            assert!(second.contains("Done"), "second: {second:?}");
            assert!(second.contains("cat README"), "second: {second:?}");
            assert_eq!(lines.next(), None);
        });
    }

    #[test]
    fn reporter_clears_state_changed_flag() {
        let mut env = Env::new_virtual();
        let index = env.jobs.add({
            let mut job = Job::new(Pid(10));
            job.state_changed = true;
            job.name = "echo hello".to_string();
            job
        });
        env.options.set(Interactive, On);
        env.options.set(Monitor, On);
        let ref_env = RefCell::new(&mut env);
        let memory = Memory::new("echo hello\n");
        let mut reporter = Reporter::new(memory, &ref_env);

        reporter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert!(!env.jobs.get(index).unwrap().state_changed);
    }

    #[test]
    fn no_report_if_not_interactive() {
        let system = Box::new(VirtualSystem::new());
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.jobs.add({
            let mut job = Job::new(Pid(10));
            job.state_changed = true;
            job.name = "echo hello".to_string();
            job
        });
        env.options.set(Monitor, On);
        let ref_env = RefCell::new(&mut env);
        let memory = Memory::new("echo hello\n");
        let mut reporter = Reporter::new(memory, &ref_env);

        reporter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn no_report_if_not_monitor() {
        let system = Box::new(VirtualSystem::new());
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.jobs.add({
            let mut job = Job::new(Pid(10));
            job.state_changed = true;
            job.name = "echo hello".to_string();
            job
        });
        env.options.set(Interactive, On);
        let ref_env = RefCell::new(&mut env);
        let memory = Memory::new("echo hello\n");
        let mut reporter = Reporter::new(memory, &ref_env);

        reporter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }
}
