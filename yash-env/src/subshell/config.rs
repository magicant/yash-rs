// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Subshell creation via [`Config`]

use super::JobControl;
use super::block_sigint_sigquit;
use super::restore_sigmask;
use crate::Env;
use crate::job::tcsetpgrp_with_block;
use crate::job::{Pid, ProcessResult};
use crate::semantics::exit_or_raise;
use crate::stack::Frame;
use crate::system::concurrency::WaitForSignals;
use crate::system::resource::SetRlimit;
use crate::system::{
    Close, Dup, Errno, Exit, Fork, GetPid, Open, SendSignal, SetPgid, Sigaction, Sigmask,
    TcSetPgrp, Wait,
};
use crate::trap::SignalSystem;

/// Configuration for subshell creation
///
/// This struct configures how a subshell is created.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct Config {
    /// Specifies disposition of the subshell with respect to job control.
    ///
    /// If this value is `None` (which is the default), the subshell runs in
    /// the same process group as the parent process. If it is `Some(_)`, the
    /// subshell becomes a new process group leader. For
    /// `Some(JobControl::Foreground)`, it also brings itself to the foreground.
    ///
    /// This parameter is ignored if the shell is not
    /// [controlling jobs](Env::controls_jobs) when starting the subshell. You
    /// can tell the actual job control status of the subshell by checking the
    /// second return value of [`start`](Self::start) in the parent environment
    /// and the second argument passed to the task in the subshell environment.
    ///
    /// If the parent process is a job-controlling interactive shell, but the
    /// subshell is not job-controlled, the subshell's signal dispositions for
    /// `SIGTSTP`, `SIGTTIN`, and `SIGTTOU` are set to `Ignore`. This is to
    /// prevent the subshell from being stopped by a job-stopping signal. Were
    /// the subshell stopped, you could never resume it since it is not
    /// job-controlled.
    pub job_control: Option<JobControl>,

    /// If `true`, the subshell ignores `SIGINT` and `SIGQUIT`.
    ///
    /// This parameter is for implementing the POSIX requirement that
    /// asynchronous and-or lists ignore `SIGINT` and `SIGQUIT` if job control
    /// is disabled. The value is passed to
    /// [`TrapSet::enter_subshell`](crate::trap::TrapSet::enter_subshell) to
    /// modify the signal dispositions for `SIGINT` and `SIGQUIT` in the
    /// subshell.
    ///
    /// This parameter has no effect if the subshell is job-controlled (see
    /// [`job_control`](Self::job_control)). The default value is `false`.
    pub ignores_sigint_sigquit: bool,
}

impl Config {
    /// Creates a new `Config` with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `Config` with foreground job control.
    ///
    /// This is a convenient function to create a `Config` for a subshell that
    /// should run in the foreground if job control is active. The returned
    /// `Config` has [`job_control`](Self::job_control) set to
    /// `Some(JobControl::Foreground)`, and the other fields set to their
    /// default values.
    #[must_use]
    pub fn foreground() -> Self {
        Self {
            job_control: Some(JobControl::Foreground),
            ..Self::default()
        }
    }

    /// Starts the subshell.
    ///
    /// This function creates a new child process that runs the task contained
    /// in this builder.
    ///
    /// Although this function is `async`, it does not wait for the child to
    /// finish, which means the parent and child processes will run
    /// concurrently. To wait for the child to change state, call
    /// [`Env::wait_for_subshell`], [`Env::wait_for_subshell_to_halt`], or
    /// [`Env::wait_for_subshell_to_finish`]. If job control is active, you may
    /// want to add the process ID to [`Env::jobs`] before waiting. To start the
    /// subshell and wait for it to finish at once, use
    /// [`start_and_wait`](Self::start_and_wait).
    ///
    /// If you set [`job_control`](Self::job_control) to
    /// `Some(JobControl::Foreground)`, this function opens [`Env::tty`] by
    /// calling [`Env::get_tty`]. The `tty` is used to change the foreground job
    /// to the new subshell. However, `job_control` is effective only when the
    /// shell is [controlling jobs](Env::controls_jobs).
    ///
    /// If the subshell started successfully, the return value is a pair of the
    /// child process ID and the actual job control status. Otherwise, it
    /// indicates the error.
    pub async fn start<S, F>(
        &self,
        env: &mut Env<S>,
        task: F,
    ) -> Result<(Pid, Option<JobControl>), Errno>
    where
        S: Close
            + Dup
            + Exit
            + Fork
            + GetPid
            + Open
            + SendSignal
            + SetPgid
            + SetRlimit
            + Sigaction
            + Sigmask
            + SignalSystem
            + TcSetPgrp
            + WaitForSignals
            + 'static,
        F: AsyncFnOnce(&mut Env<S>, Option<JobControl>) + 'static,
    {
        // Do some preparation before starting a child process
        let job_control = env.controls_jobs().then_some(self.job_control).flatten();
        let tty = match job_control {
            None | Some(JobControl::Background) => None,
            // Open the tty in the parent process so we can reuse the FD for other jobs
            Some(JobControl::Foreground) => env.get_tty().await.ok(),
        };

        let ignore_sigint_sigquit = self.ignores_sigint_sigquit && job_control.is_none();
        let original_mask = if ignore_sigint_sigquit {
            // Block SIGINT and SIGQUIT before forking the child process to
            // prevent the child from being killed by those signals until the
            // child starts ignoring them.
            Some(block_sigint_sigquit(&env.system).await?)
        } else {
            None
        };
        let keep_internal_dispositions_for_stoppers = job_control.is_none();

        // Define the child process task
        const ME: Pid = Pid(0);
        let child_task = move |mut child_env: Env<S>, ()| async move {
            let env = &mut *child_env.push_frame(Frame::Subshell);

            if let Some(job_control) = job_control {
                if let Ok(()) = env.system.setpgid(ME, ME) {
                    match job_control {
                        JobControl::Background => (),
                        JobControl::Foreground => {
                            if let Some(tty) = tty {
                                let pgid = env.system.getpgrp();
                                tcsetpgrp_with_block(&env.system, tty, pgid).await.ok();
                            }
                        }
                    }
                }
            }
            env.jobs.disown_all();

            env.traps
                .enter_subshell(
                    &env.system,
                    ignore_sigint_sigquit,
                    keep_internal_dispositions_for_stoppers,
                )
                .await;

            task(env, job_control).await;
            exit_or_raise(&env.system, env.exit_status).await
        };

        // Start the child
        let (result, ()) = env.run_in_child_process((), child_task);

        // Restore the original signal mask in the parent process. Need to do
        // this before returning the error if the child process creation failed,
        // to avoid leaving the parent process with an unexpected signal mask.
        if let Some(mask) = original_mask {
            restore_sigmask(&env.system, &mask).await.ok();
        }

        let child_pid = result?;

        // The finishing
        if job_control.is_some() {
            // We should setpgid not only in the child but also in the parent to
            // make sure the child is in a new process group before the parent
            // returns from the start function.
            let _ = env.system.setpgid(child_pid, ME);

            // We don't tcsetpgrp in the parent. It would mess up the child
            // which may have started another shell doing its own job control.
        }

        Ok((child_pid, job_control))
    }

    /// Starts the subshell and waits for it to finish.
    ///
    /// This function [starts](Self::start) `self` and
    /// [waits](Env::wait_for_subshell) for it to finish. This function returns
    /// when the subshell process exits or is killed by a signal. If the
    /// subshell is job-controlled, the function also returns when the job is
    /// suspended.
    ///
    /// If the subshell started successfully, the return value is the process ID
    /// and the process result of the subshell. If there was an error starting
    /// the subshell, this function returns the error.
    ///
    /// If you set [`job_control`](Self::job_control) to
    /// `JobControl::Foreground` and job control is effective as per
    /// [`Env::controls_jobs`], this function makes the shell the foreground job
    /// after the subshell terminated or suspended.
    ///
    /// When a job-controlled subshell suspends, this function does not add it
    /// to `env.jobs`. You have to do it for yourself if necessary.
    pub async fn start_and_wait<S, F>(
        &self,
        env: &mut Env<S>,
        task: F,
    ) -> Result<(Pid, ProcessResult), Errno>
    where
        S: Close
            + Dup
            + Exit
            + Fork
            + GetPid
            + Open
            + SendSignal
            + SetPgid
            + SetRlimit
            + Sigaction
            + Sigmask
            + SignalSystem
            + TcSetPgrp
            + Wait
            + WaitForSignals
            + 'static,
        F: AsyncFnOnce(&mut Env<S>, Option<JobControl>) + 'static,
    {
        let (pid, job_control) = self.start(env, task).await?;
        let result = loop {
            let result = env.wait_for_subshell_to_halt(pid).await?.1;
            if !result.is_stopped() || job_control.is_some() {
                break result;
            }
        };

        if job_control == Some(JobControl::Foreground) {
            if let Some(tty) = env.tty {
                tcsetpgrp_with_block(&env.system, tty, env.main_pgid)
                    .await
                    .ok();
            }
        }

        Ok((pid, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{Job, ProcessState};
    use crate::option::Option::{Interactive, Monitor};
    use crate::option::State::On;
    use crate::semantics::ExitStatus;
    use crate::source::Location;
    use crate::system::r#virtual::{Inode, SystemState, VirtualSystem};
    use crate::system::r#virtual::{SIGCHLD, SIGINT, SIGQUIT, SIGTSTP, SIGTTIN, SIGTTOU};
    use crate::system::{Concurrent, Disposition};
    use crate::test_helper::in_virtual_system;
    use crate::trap::Action;
    use assert_matches::assert_matches;
    use futures_executor::LocalPool;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn stub_tty(state: &RefCell<SystemState>) {
        state
            .borrow_mut()
            .file_system
            .save("/dev/tty", Rc::new(RefCell::new(Inode::new([]))))
            .unwrap();
    }

    #[test]
    fn start_returns_child_process_id() {
        in_virtual_system(|mut env, _state| async move {
            let parent_pid = env.main_pid;
            let child_pid = Rc::new(Cell::new(None));
            let child_pid_2 = Rc::clone(&child_pid);
            let result = Config::new()
                .start(
                    &mut env,
                    async move |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        child_pid_2.set(Some(env.system.getpid()));
                        assert_eq!(env.system.getppid(), parent_pid);
                    },
                )
                .await
                .unwrap()
                .0;
            env.wait_for_subshell(result).await.unwrap();
            assert_eq!(Some(result), child_pid.get());
        });
    }

    #[test]
    fn start_failing() {
        let mut executor = LocalPool::new();
        let env = &mut Env::new_virtual();
        let result = executor.run_until(
            Config::new().start(env, async |_env: &mut Env<_>, _job_control| {
                unreachable!("subshell not expected to run")
            }),
        );
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn stack_frame_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            let pid = Config::new()
                .start(
                    &mut env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        assert_eq!(env.stack[..], [Frame::Subshell])
                    },
                )
                .await
                .unwrap()
                .0;
            assert_eq!(env.stack[..], []);

            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn jobs_disowned_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            let index = env.jobs.add(Job::new(Pid(123)));
            let pid = Config::new()
                .start(
                    &mut env,
                    async move |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        assert!(!env.jobs[index].is_owned)
                    },
                )
                .await
                .unwrap()
                .0;
            env.wait_for_subshell(pid).await.unwrap();

            assert!(env.jobs[index].is_owned);
        });
    }

    #[test]
    fn trap_reset_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            env.traps
                .set_action(
                    &env.system,
                    SIGCHLD,
                    Action::Command("echo foo".into()),
                    Location::dummy(""),
                    false,
                )
                .await
                .unwrap();
            let pid = Config::new()
                .start(
                    &mut env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        let (current, parent) = env.traps.get_state(SIGCHLD);
                        assert_eq!(current.unwrap().action, Action::Default);
                        assert_matches!(
                            &parent.unwrap().action,
                            Action::Command(body) => assert_eq!(&**body, "echo foo")
                        );
                    },
                )
                .await
                .unwrap()
                .0;
            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn subshell_with_no_job_control() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);

            let parent_pgid = state.borrow().processes[&parent_env.main_pid].pgid;
            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Config::new()
                .start(
                    &mut parent_env,
                    async move |child_env: &mut Env<Rc<Concurrent<VirtualSystem>>>, job_control| {
                        let child_pid = child_env.system.getpid();
                        assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                        assert_eq!(state_2.borrow().foreground, None);
                        assert_eq!(job_control, None);
                    },
                )
                .await
                .unwrap();
            assert_eq!(job_control, None);
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);
        });
    }

    #[test]
    fn subshell_in_background() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);

            let state_2 = Rc::clone(&state);
            let config = Config {
                job_control: Some(JobControl::Background),
                ..Config::new()
            };
            let (child_pid, job_control) = config
                .start(
                    &mut parent_env,
                    async move |child_env: &mut Env<Rc<Concurrent<VirtualSystem>>>, job_control| {
                        let child_pid = child_env.system.getpid();
                        assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                        assert_eq!(state_2.borrow().foreground, None);
                        assert_eq!(job_control, Some(JobControl::Background));
                    },
                )
                .await
                .unwrap();
            assert_eq!(job_control, Some(JobControl::Background));
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
            assert_eq!(state.borrow().foreground, None);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
            assert_eq!(state.borrow().foreground, None);
        });
    }

    #[test]
    fn subshell_in_foreground() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Config::foreground()
                .start(
                    &mut parent_env,
                    async move |child_env: &mut Env<Rc<Concurrent<VirtualSystem>>>, job_control| {
                        let child_pid = child_env.system.getpid();
                        assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                        assert_eq!(state_2.borrow().foreground, Some(child_pid));
                        assert_eq!(job_control, Some(JobControl::Foreground));
                    },
                )
                .await
                .unwrap();
            assert_eq!(job_control, Some(JobControl::Foreground));
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
            // The child may not yet have become the foreground job.
            // assert_eq!(state.borrow().foreground, Some(child_pid));

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
            assert_eq!(state.borrow().foreground, Some(child_pid));
        });
    }

    #[test]
    fn tty_after_starting_foreground_subshell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let _ = Config::foreground()
                .start(
                    &mut parent_env,
                    async move |_: &mut Env<Rc<Concurrent<VirtualSystem>>>, _| (),
                )
                .await
                .unwrap();
            assert_matches!(parent_env.tty, Some(_));
        });
    }

    #[test]
    fn job_control_without_tty() {
        // When /dev/tty is not available, the shell cannot bring the subshell to
        // the foreground. The subshell should still be in a new process group.
        // This is the behavior required by POSIX.
        in_virtual_system(async |mut parent_env, state| {
            parent_env.options.set(Monitor, On);

            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Config::foreground()
                .start(
                    &mut parent_env,
                    async move |child_env: &mut Env<Rc<Concurrent<VirtualSystem>>>, job_control| {
                        let child_pid = child_env.system.getpid();
                        assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                        assert_eq!(job_control, Some(JobControl::Foreground));
                    },
                )
                .await
                .unwrap();
            assert_eq!(job_control, Some(JobControl::Foreground));
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
        })
    }

    #[test]
    fn no_job_control_with_option_disabled() {
        in_virtual_system(|mut parent_env, state| async move {
            stub_tty(&state);

            let parent_pgid = state.borrow().processes[&parent_env.main_pid].pgid;
            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Config::foreground()
                .start(
                    &mut parent_env,
                    async move |child_env: &mut Env<Rc<Concurrent<VirtualSystem>>>,
                                _job_control| {
                        let child_pid = child_env.system.getpid();
                        assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                        assert_eq!(state_2.borrow().foreground, None);
                    },
                )
                .await
                .unwrap();
            assert_eq!(job_control, None);
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);
        });
    }

    #[test]
    fn no_job_control_for_nested_subshell() {
        in_virtual_system(|mut parent_env, state| async move {
            let mut parent_env = parent_env.push_frame(Frame::Subshell);
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let parent_pgid = state.borrow().processes[&parent_env.main_pid].pgid;
            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Config::foreground()
                .start(
                    &mut parent_env,
                    async move |child_env: &mut Env<Rc<Concurrent<VirtualSystem>>>,
                                _job_control| {
                        let child_pid = child_env.system.getpid();
                        assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                        assert_eq!(state_2.borrow().foreground, None);
                    },
                )
                .await
                .unwrap();
            assert_eq!(job_control, None);
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);
        });
    }

    #[test]
    fn wait_without_job_control() {
        in_virtual_system(|mut env, _state| async move {
            let (_pid, process_result) = Config::new()
                .start_and_wait(
                    &mut env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        env.exit_status = ExitStatus(42)
                    },
                )
                .await
                .unwrap();
            assert_eq!(process_result, ProcessResult::exited(42));
        });
    }

    #[test]
    fn wait_for_foreground_job_to_exit() {
        in_virtual_system(|mut env, state| async move {
            env.options.set(Monitor, On);
            stub_tty(&state);

            let (_pid, process_result) = Config::foreground()
                .start_and_wait(
                    &mut env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        env.exit_status = ExitStatus(123)
                    },
                )
                .await
                .unwrap();
            assert_eq!(process_result, ProcessResult::exited(123));
            assert_eq!(state.borrow().foreground, Some(env.main_pgid));
        });
    }

    // TODO wait_for_foreground_job_to_be_signaled
    // TODO wait_for_foreground_job_to_be_stopped

    #[test]
    fn sigint_sigquit_not_ignored_by_default() {
        in_virtual_system(|mut parent_env, state| async move {
            let (child_pid, _) = Config {
                job_control: Some(JobControl::Background),
                ..Config::new()
            }
            .start(
                &mut parent_env,
                async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                    env.exit_status = ExitStatus(123)
                },
            )
            .await
            .unwrap();
            parent_env.wait_for_subshell(child_pid).await.unwrap();

            let state = state.borrow();
            let process = &state.processes[&child_pid];
            assert_eq!(process.disposition(SIGINT), Disposition::Default);
            assert_eq!(process.disposition(SIGQUIT), Disposition::Default);
        })
    }

    #[test]
    fn sigint_sigquit_ignored_in_uncontrolled_job() {
        in_virtual_system(|mut parent_env, state| async move {
            let (child_pid, _) = Config {
                job_control: Some(JobControl::Background),
                ignores_sigint_sigquit: true,
            }
            .start(
                &mut parent_env,
                async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                    env.exit_status = ExitStatus(123)
                },
            )
            .await
            .unwrap();

            parent_env
                .system
                .kill(child_pid, Some(SIGINT))
                .await
                .unwrap();
            parent_env
                .system
                .kill(child_pid, Some(SIGQUIT))
                .await
                .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(child_result, (child_pid, ProcessState::exited(123)));

            let state = state.borrow();
            let parent_process = &state.processes[&parent_env.main_pid];
            assert!(!parent_process.blocked_signals().contains(&SIGINT));
            assert!(!parent_process.blocked_signals().contains(&SIGQUIT));
            let child_process = &state.processes[&child_pid];
            assert_eq!(child_process.disposition(SIGINT), Disposition::Ignore);
            assert_eq!(child_process.disposition(SIGQUIT), Disposition::Ignore);
        })
    }

    #[test]
    fn sigint_sigquit_not_ignored_if_job_controlled() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let (child_pid, _) = Config {
                job_control: Some(JobControl::Background),
                ignores_sigint_sigquit: true,
            }
            .start(
                &mut parent_env,
                async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                    env.exit_status = ExitStatus(123)
                },
            )
            .await
            .unwrap();
            parent_env.wait_for_subshell(child_pid).await.unwrap();

            let state = state.borrow();
            let process = &state.processes[&child_pid];
            assert_eq!(process.disposition(SIGINT), Disposition::Default);
            assert_eq!(process.disposition(SIGQUIT), Disposition::Default);
        })
    }

    #[test]
    fn internal_dispositions_for_stoppers_kept_in_uncontrolled_subshell_of_controlling_interactive_shell()
     {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Interactive, On);
            parent_env.options.set(Monitor, On);
            parent_env
                .traps
                .enable_internal_dispositions_for_stoppers(&parent_env.system)
                .await
                .unwrap();
            stub_tty(&state);

            let (child_pid, _) = Config::new()
                .start(
                    &mut parent_env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        env.exit_status = ExitStatus(123)
                    },
                )
                .await
                .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(child_result, (child_pid, ProcessState::exited(123)));

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(child_process.disposition(SIGTSTP), Disposition::Ignore);
            assert_eq!(child_process.disposition(SIGTTIN), Disposition::Ignore);
            assert_eq!(child_process.disposition(SIGTTOU), Disposition::Ignore);
        })
    }

    #[test]
    fn internal_dispositions_for_stoppers_reset_in_controlled_subshell_of_interactive_shell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Interactive, On);
            parent_env.options.set(Monitor, On);
            parent_env
                .traps
                .enable_internal_dispositions_for_stoppers(&parent_env.system)
                .await
                .unwrap();
            stub_tty(&state);

            let (child_pid, _) = Config {
                job_control: Some(JobControl::Background),
                ..Config::new()
            }
            .start(
                &mut parent_env,
                async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                    env.exit_status = ExitStatus(123)
                },
            )
            .await
            .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(child_result, (child_pid, ProcessState::exited(123)));

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(child_process.disposition(SIGTSTP), Disposition::Default);
            assert_eq!(child_process.disposition(SIGTTIN), Disposition::Default);
            assert_eq!(child_process.disposition(SIGTTOU), Disposition::Default);
        })
    }

    #[test]
    fn internal_dispositions_for_stoppers_unset_in_subshell_of_non_controlling_interactive_shell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Interactive, On);
            stub_tty(&state);

            let (child_pid, _) = Config::new()
                .start(
                    &mut parent_env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        env.exit_status = ExitStatus(123)
                    },
                )
                .await
                .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(child_result, (child_pid, ProcessState::exited(123)));

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(child_process.disposition(SIGTSTP), Disposition::Default);
            assert_eq!(child_process.disposition(SIGTTIN), Disposition::Default);
            assert_eq!(child_process.disposition(SIGTTOU), Disposition::Default);
        })
    }

    #[test]
    fn internal_dispositions_for_stoppers_unset_in_uncontrolled_subshell_of_controlling_non_interactive_shell()
     {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let (child_pid, _) = Config::new()
                .start(
                    &mut parent_env,
                    async |env: &mut Env<Rc<Concurrent<VirtualSystem>>>, _job_control| {
                        env.exit_status = ExitStatus(123)
                    },
                )
                .await
                .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(child_result, (child_pid, ProcessState::exited(123)));

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(child_process.disposition(SIGTSTP), Disposition::Default);
            assert_eq!(child_process.disposition(SIGTTIN), Disposition::Default);
            assert_eq!(child_process.disposition(SIGTTOU), Disposition::Default);
        })
    }
}
