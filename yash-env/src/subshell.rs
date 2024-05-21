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

//! Utility for starting subshells
//!
//! This module defines [`Subshell`], a builder for starting a subshell. It is
//! [constructed](Subshell::new) with a function you want to run in a subshell.
//! After configuring the builder with some options, you can
//! [start](Subshell::start) the subshell.
//!
//! [`Subshell`] is implemented as a wrapper around
//! [`System::new_child_process`]. You should prefer `Subshell` for the purpose
//! of creating a subshell because it helps to arrange the child process
//! properly.

use crate::job::Pid;
use crate::job::ProcessState;
use crate::stack::Frame;
use crate::system::ChildProcessTask;
use crate::system::Errno;
use crate::system::SigSet;
use crate::system::SigmaskHow::{SIG_BLOCK, SIG_SETMASK};
use crate::system::System;
use crate::system::SystemEx;
use crate::trap::Signal::{SIGINT, SIGQUIT};
use crate::Env;
use std::future::Future;
use std::pin::Pin;

/// Job state of a newly created subshell
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobControl {
    /// The subshell becomes the foreground process group.
    Foreground,
    /// The subshell becomes a background process group.
    Background,
}

/// Subshell builder
///
/// See the [module documentation](self) for details.
#[must_use = "a subshell is not started unless you call `Subshell::start`"]
pub struct Subshell<F> {
    task: F,
    job_control: Option<JobControl>,
    ignores_sigint_sigquit: bool,
}

impl<F> std::fmt::Debug for Subshell<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subshell").finish_non_exhaustive()
    }
}

impl<F> Subshell<F>
where
    F: for<'a> FnOnce(&'a mut Env, Option<JobControl>) -> Pin<Box<dyn Future<Output = ()> + 'a>>
        + 'static,
    // TODO Revisit to simplify this function type when impl Future is allowed in return type
{
    /// Creates a new subshell builder with a task.
    ///
    /// The task will run in a subshell after it is started. The task takes two
    /// arguments:
    ///
    /// 1. The environment in which the subshell runs, and
    /// 2. Job control status for the subshell.
    ///
    /// If the task returns an `Err(Divert::...)`, it is handled as follows:
    ///
    /// - `Interrupt` and `Exit` with `Some(exit_status)` override the exit
    ///   status in `Env`.
    /// - Other `Divert` values are ignored.
    pub fn new(task: F) -> Self {
        Subshell {
            task,
            job_control: None,
            ignores_sigint_sigquit: false,
        }
    }

    /// Specifies disposition of the subshell with respect to job control.
    ///
    /// If the argument is `None` (which is the default), the subshell runs in
    /// the same process group as the parent process. If it is `Some(_)`, the
    /// subshell becomes a new process group. For `JobControl::Foreground`, it
    /// also brings itself to the foreground.
    ///
    /// This parameter is ignored if the shell is not [controlling
    /// jobs](Env::controls_jobs) when starting the subshell. You can tell the
    /// actual job control status of the subshell by the second return value of
    /// [`start`](Self::start) in the parent environment and the second argument
    /// passed to the task in the subshell environment.
    ///
    /// If the parent process is a job-controlling interactive shell, but the
    /// subshell is not job-controlled, the subshell's signal handlings for
    /// SIGTSTP, SIGTTIN, and SIGTTOU are set to `Ignore`. This is to prevent
    /// the subshell from being stopped by a job-stopping signal. Were the
    /// subshell stopped, you could never resume it since it is not
    /// job-controlled.
    pub fn job_control<J: Into<Option<JobControl>>>(mut self, job_control: J) -> Self {
        self.job_control = job_control.into();
        self
    }

    /// Makes the subshell ignore SIGINT and SIGQUIT.
    ///
    /// If `ignore` is true and the subshell is not job-controlled, the subshell
    /// sets its signal handlings for SIGINT and SIGQUIT to `Ignore`.
    ///
    /// The default is `false`.
    pub fn ignore_sigint_sigquit(mut self, ignore: bool) -> Self {
        self.ignores_sigint_sigquit = ignore;
        self
    }

    /// Starts the subshell.
    ///
    /// This function creates a new child process that runs the task contained
    /// in this builder.
    ///
    /// Although this function is `async`, it does not wait for the child to
    /// finish, which means the parent and child processes will run
    /// concurrently. To wait for the child to finish, you need to call
    /// [`Env::wait_for_subshell`] or [`Env::wait_for_subshell_to_finish`]. If
    /// job control is active, you may want to add the process ID to `env.jobs`
    /// before waiting.
    ///
    /// If you set [`job_control`](Self::job_control) to
    /// `JobControl::Foreground`, this function opens `env.tty` by calling
    /// [`Env::get_tty`]. The `tty` is used to change the foreground job to the
    /// new subshell. However, `job_control` is effective only when the shell is
    /// [controlling jobs](Env::controls_jobs).
    ///
    /// If the subshell started successfully, the return value is a pair of the
    /// child process ID and the actual job control. Otherwise, it indicates the
    /// error.
    pub async fn start(self, env: &mut Env) -> Result<(Pid, Option<JobControl>), Errno> {
        // Do some preparation before starting a child process
        let job_control = env.controls_jobs().then_some(self.job_control).flatten();
        let tty = match job_control {
            None | Some(JobControl::Background) => None,
            // Open the tty in the parent process so we can reuse the FD for other jobs
            Some(JobControl::Foreground) => Some(env.get_tty()?),
        };
        // Block SIGINT and SIGQUIT before forking the child process to prevent
        // the child from being killed by those signals until the child starts
        // ignoring them.
        let mut mask_guard = MaskGuard::new(env);
        let ignores_sigint_sigquit = self.ignores_sigint_sigquit
            && job_control.is_none()
            && mask_guard.block_sigint_sigquit();
        let keeps_stopper_handlers = job_control.is_none();

        // Define the child process task
        const ME: Pid = Pid(0);
        let task: ChildProcessTask = Box::new(move |env| {
            Box::pin(async move {
                let mut env = env.push_frame(Frame::Subshell);
                let env = &mut *env;

                if let Some(job_control) = job_control {
                    if let Ok(()) = env.system.setpgid(ME, ME) {
                        match job_control {
                            JobControl::Background => (),
                            JobControl::Foreground => {
                                if let Some(tty) = tty {
                                    let pgid = env.system.getpgrp();
                                    let _ = env.system.tcsetpgrp_with_block(tty, pgid);
                                }
                            }
                        }
                    }
                }
                env.jobs.disown_all();

                env.traps.enter_subshell(
                    &mut env.system,
                    ignores_sigint_sigquit,
                    keeps_stopper_handlers,
                );

                (self.task)(env, job_control).await
            })
        });

        // Start the child
        let child = mask_guard.env.system.new_child_process()?;
        let child_pid = child(mask_guard.env, task).await;

        // The finishing
        if job_control.is_some() {
            // We should setpgid not only in the child but also in the parent to
            // make sure the child is in a new process group before the parent
            // returns from the start function.
            let _ = mask_guard.env.system.setpgid(child_pid, ME);

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
    /// and the process state of the subshell, which is `Exited`, `Signaled`, or
    /// `Stopped`. If there was an error starting the subshell, this function
    /// returns the error.
    ///
    /// If you set [`job_control`](Self::job_control) to
    /// `JobControl::Foreground` and job control is effective as per
    /// [`Env::controls_jobs`], this function makes the shell the foreground job
    /// after the subshell terminated or suspended.
    ///
    /// When a job-controlled subshell suspends, this function does not add it
    /// to `env.jobs`. You have to do it for yourself if necessary.
    pub async fn start_and_wait(self, env: &mut Env) -> Result<(Pid, ProcessState), Errno> {
        let (pid, job_control) = self.start(env).await?;
        let result = loop {
            let (pid, state) = env.wait_for_subshell(pid).await?;
            let is_done = match state {
                ProcessState::Running => false,
                _ => todo!(),
                // ProcessState::Stopped(_) => job_control.is_some(),
                // ProcessState::Exited(_) | ProcessState::Signaled { .. } => true,
            };
            if is_done {
                break Ok((pid, state));
            }
        };

        if job_control == Some(JobControl::Foreground) {
            if let Some(tty) = env.tty {
                env.system.tcsetpgrp_with_block(tty, env.main_pgid).ok();
            }
        }

        result
    }
}

/// Guard object for temporarily blocking signals
///
/// This object blocks SIGINT and SIGQUIT and remembers the previous signal
/// blocking mask, which is restored when the object is dropped.
#[derive(Debug)]
struct MaskGuard<'a> {
    env: &'a mut Env,
    old_mask: Option<SigSet>,
}

impl<'a> MaskGuard<'a> {
    fn new(env: &'a mut Env) -> Self {
        let old_mask = None;
        Self { env, old_mask }
    }

    fn block_sigint_sigquit(&mut self) -> bool {
        assert_eq!(self.old_mask, None);

        let mut sigint_sigquit = SigSet::empty();
        let mut old_mask = SigSet::empty();
        sigint_sigquit.add(SIGINT);
        sigint_sigquit.add(SIGQUIT);

        let success = self
            .env
            .system
            .sigmask(SIG_BLOCK, Some(&sigint_sigquit), Some(&mut old_mask))
            .is_ok();
        if success {
            self.old_mask = Some(old_mask);
        }
        success
    }
}

impl<'a> Drop for MaskGuard<'a> {
    fn drop(&mut self) {
        if let Some(old_mask) = &self.old_mask {
            self.env
                .system
                .sigmask(SIG_SETMASK, Some(old_mask), None)
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::Job;
    use crate::option::Option::{Interactive, Monitor};
    use crate::option::State::On;
    use crate::semantics::ExitStatus;
    use crate::system::r#virtual::INode;
    use crate::system::r#virtual::SystemState;
    use crate::system::Errno;
    use crate::system::SignalHandling;
    use crate::tests::in_virtual_system;
    use crate::trap::Action;
    use crate::trap::Signal::SIGCHLD;
    use assert_matches::assert_matches;
    use futures_executor::LocalPool;
    use nix::sys::signal::Signal::{SIGTSTP, SIGTTIN, SIGTTOU};
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_syntax::source::Location;

    fn stub_tty(state: &RefCell<SystemState>) {
        state
            .borrow_mut()
            .file_system
            .save("/dev/tty", Rc::new(RefCell::new(INode::new([]))))
            .unwrap();
    }

    #[test]
    fn subshell_start_returns_child_process_id() {
        in_virtual_system(|mut env, _state| async move {
            let parent_pid = env.main_pid;
            let child_pid = Rc::new(Cell::new(None));
            let child_pid_2 = Rc::clone(&child_pid);
            let subshell = Subshell::new(move |env, _job_control| {
                Box::pin(async move {
                    child_pid_2.set(Some(env.system.getpid()));
                    assert_eq!(env.system.getppid(), parent_pid);
                })
            });
            let result = subshell.start(&mut env).await.unwrap().0;
            env.wait_for_subshell(result).await.unwrap();
            assert_eq!(Some(result), child_pid.get());
        });
    }

    #[test]
    fn subshell_start_failing() {
        let mut executor = LocalPool::new();
        let env = &mut Env::new_virtual();
        let subshell =
            Subshell::new(|_env, _job_control| unreachable!("subshell not expected to run"));
        let result = executor.run_until(subshell.start(env));
        assert_eq!(result, Err(Errno::ENOSYS));
    }

    #[test]
    fn stack_frame_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            let subshell = Subshell::new(|env, _job_control| {
                Box::pin(async { assert_eq!(env.stack[..], [Frame::Subshell]) })
            });
            let pid = subshell.start(&mut env).await.unwrap().0;
            assert_eq!(env.stack[..], []);

            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn jobs_disowned_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            let index = env.jobs.add(Job::new(Pid(123)));
            let subshell = Subshell::new(move |env, _job_control| {
                Box::pin(async move { assert!(!env.jobs[index].is_owned) })
            });
            let pid = subshell.start(&mut env).await.unwrap().0;
            env.wait_for_subshell(pid).await.unwrap();

            assert!(env.jobs[index].is_owned);
        });
    }

    #[test]
    fn trap_reset_in_subshell() {
        in_virtual_system(|mut env, _state| async move {
            env.traps
                .set_action(
                    &mut env.system,
                    SIGCHLD,
                    Action::Command("echo foo".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            let subshell = Subshell::new(|env, _job_control| {
                Box::pin(async {
                    let trap_state = assert_matches!(
                        env.traps.get_state(SIGCHLD),
                        (None, Some(trap_state)) => trap_state
                    );
                    assert_matches!(
                        &trap_state.action,
                        Action::Command(body) => assert_eq!(&**body, "echo foo")
                    );
                })
            });
            let pid = subshell.start(&mut env).await.unwrap().0;
            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn subshell_with_no_job_control() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);

            let parent_pgid = state.borrow().processes[&parent_env.main_pid].pgid;
            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Subshell::new(move |child_env, job_control| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                    assert_eq!(state_2.borrow().foreground, None);
                    assert_eq!(job_control, None);
                })
            })
            .job_control(None)
            .start(&mut parent_env)
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
            let (child_pid, job_control) = Subshell::new(move |child_env, job_control| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                    assert_eq!(state_2.borrow().foreground, None);
                    assert_eq!(job_control, Some(JobControl::Background));
                })
            })
            .job_control(JobControl::Background)
            .start(&mut parent_env)
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
            let (child_pid, job_control) = Subshell::new(move |child_env, job_control| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                    assert_eq!(state_2.borrow().foreground, Some(child_pid));
                    assert_eq!(job_control, Some(JobControl::Foreground));
                })
            })
            .job_control(JobControl::Foreground)
            .start(&mut parent_env)
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

            let _ = Subshell::new(move |_, _| Box::pin(std::future::ready(())))
                .job_control(JobControl::Foreground)
                .start(&mut parent_env)
                .await
                .unwrap();
            assert_matches!(parent_env.tty, Some(_));
        });
    }

    #[test]
    fn no_job_control_with_option_disabled() {
        in_virtual_system(|mut parent_env, state| async move {
            stub_tty(&state);

            let parent_pgid = state.borrow().processes[&parent_env.main_pid].pgid;
            let state_2 = Rc::clone(&state);
            let (child_pid, job_control) = Subshell::new(move |child_env, _job_control| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                    assert_eq!(state_2.borrow().foreground, None);
                })
            })
            .job_control(JobControl::Foreground)
            .start(&mut parent_env)
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
            let (child_pid, job_control) = Subshell::new(move |child_env, _job_control| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                    assert_eq!(state_2.borrow().foreground, None);
                })
            })
            .job_control(JobControl::Foreground)
            .start(&mut parent_env)
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
            let subshell = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(42) })
            });
            let (_pid, process_state) = subshell.start_and_wait(&mut env).await.unwrap();
            // TODO assert_eq!(process_state, ProcessState::Exited(ExitStatus(42)));
        });
    }

    #[test]
    fn wait_for_foreground_job_to_exit() {
        in_virtual_system(|mut env, state| async move {
            env.options.set(Monitor, On);
            stub_tty(&state);

            let subshell = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .job_control(JobControl::Foreground);
            let (_pid, process_state) = subshell.start_and_wait(&mut env).await.unwrap();
            // TODO assert_eq!(process_state, ProcessState::Exited(ExitStatus(123)));
            assert_eq!(state.borrow().foreground, Some(env.main_pgid));
        });
    }

    // TODO wait_for_foreground_job_to_be_signaled
    // TODO wait_for_foreground_job_to_be_stopped

    #[test]
    fn sigint_sigquit_not_ignored_by_default() {
        in_virtual_system(|mut parent_env, state| async move {
            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .job_control(JobControl::Background)
            .start(&mut parent_env)
            .await
            .unwrap();
            parent_env.wait_for_subshell(child_pid).await.unwrap();

            let state = state.borrow();
            let process = &state.processes[&child_pid];
            assert_eq!(process.signal_handling(SIGINT), SignalHandling::Default);
            assert_eq!(process.signal_handling(SIGQUIT), SignalHandling::Default);
        })
    }

    #[test]
    fn sigint_sigquit_ignored_in_uncontrolled_job() {
        in_virtual_system(|mut parent_env, state| async move {
            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .job_control(JobControl::Background)
            .ignore_sigint_sigquit(true)
            .start(&mut parent_env)
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
            // TODO
            // assert_eq!(
            //     child_result,
            //     (child_pid, ProcessState::Exited(ExitStatus(123)))
            // );

            let state = state.borrow();
            let parent_process = &state.processes[&parent_env.main_pid];
            assert!(!parent_process.blocked_signals().contains(SIGINT));
            assert!(!parent_process.blocked_signals().contains(SIGQUIT));
            let child_process = &state.processes[&child_pid];
            assert_eq!(
                child_process.signal_handling(SIGINT),
                SignalHandling::Ignore
            );
            assert_eq!(
                child_process.signal_handling(SIGQUIT),
                SignalHandling::Ignore
            );
        })
    }

    #[test]
    fn sigint_sigquit_not_ignored_if_job_controlled() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .job_control(JobControl::Background)
            .ignore_sigint_sigquit(true)
            .start(&mut parent_env)
            .await
            .unwrap();
            parent_env.wait_for_subshell(child_pid).await.unwrap();

            let state = state.borrow();
            let process = &state.processes[&child_pid];
            assert_eq!(process.signal_handling(SIGINT), SignalHandling::Default);
            assert_eq!(process.signal_handling(SIGQUIT), SignalHandling::Default);
        })
    }

    #[test]
    fn stopper_handlers_kept_in_uncontrolled_subshell_of_controlling_interactive_shell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Interactive, On);
            parent_env.options.set(Monitor, On);
            parent_env
                .traps
                .enable_stopper_handlers(&mut parent_env.system)
                .unwrap();
            stub_tty(&state);

            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .start(&mut parent_env)
            .await
            .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            // TODO
            // assert_eq!(
            //     child_result,
            //     (child_pid, ProcessState::Exited(ExitStatus(123)))
            // );

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(
                child_process.signal_handling(SIGTSTP),
                SignalHandling::Ignore
            );
            assert_eq!(
                child_process.signal_handling(SIGTTIN),
                SignalHandling::Ignore
            );
            assert_eq!(
                child_process.signal_handling(SIGTTOU),
                SignalHandling::Ignore
            );
        })
    }

    #[test]
    fn stopper_handlers_reset_in_controlled_subshell_of_controlling_interactive_shell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Interactive, On);
            parent_env.options.set(Monitor, On);
            parent_env
                .traps
                .enable_stopper_handlers(&mut parent_env.system)
                .unwrap();
            stub_tty(&state);

            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .job_control(JobControl::Background)
            .start(&mut parent_env)
            .await
            .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            // TODO
            // assert_eq!(
            //     child_result,
            //     (child_pid, ProcessState::Exited(ExitStatus(123)))
            // );

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(
                child_process.signal_handling(SIGTSTP),
                SignalHandling::Default
            );
            assert_eq!(
                child_process.signal_handling(SIGTTIN),
                SignalHandling::Default
            );
            assert_eq!(
                child_process.signal_handling(SIGTTOU),
                SignalHandling::Default
            );
        })
    }

    #[test]
    fn stopper_handlers_not_set_in_subshell_of_non_controlling_interactive_shell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Interactive, On);
            stub_tty(&state);

            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .start(&mut parent_env)
            .await
            .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            // TODO
            // assert_eq!(
            //     child_result,
            //     (child_pid, ProcessState::Exited(ExitStatus(123)))
            // );

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(
                child_process.signal_handling(SIGTSTP),
                SignalHandling::Default
            );
            assert_eq!(
                child_process.signal_handling(SIGTTIN),
                SignalHandling::Default
            );
            assert_eq!(
                child_process.signal_handling(SIGTTOU),
                SignalHandling::Default
            );
        })
    }

    #[test]
    fn stopper_handlers_not_set_in_uncontrolled_subshell_of_controlling_non_interactive_shell() {
        in_virtual_system(|mut parent_env, state| async move {
            parent_env.options.set(Monitor, On);
            stub_tty(&state);

            let (child_pid, _) = Subshell::new(|env, _job_control| {
                Box::pin(async { env.exit_status = ExitStatus(123) })
            })
            .start(&mut parent_env)
            .await
            .unwrap();

            let child_result = parent_env.wait_for_subshell(child_pid).await.unwrap();
            // TODO
            // assert_eq!(
            //     child_result,
            //     (child_pid, ProcessState::Exited(ExitStatus(123)))
            // );

            let state = state.borrow();
            let child_process = &state.processes[&child_pid];
            assert_eq!(
                child_process.signal_handling(SIGTSTP),
                SignalHandling::Default
            );
            assert_eq!(
                child_process.signal_handling(SIGTTIN),
                SignalHandling::Default
            );
            assert_eq!(
                child_process.signal_handling(SIGTTOU),
                SignalHandling::Default
            );
        })
    }
}
