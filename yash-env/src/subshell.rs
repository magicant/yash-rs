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
use crate::stack::Frame;
use crate::system::ChildProcessTask;
use crate::system::System;
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
}

impl<F> std::fmt::Debug for Subshell<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subshell").finish_non_exhaustive()
    }
}

impl<F> Subshell<F>
where
    F: for<'a> FnOnce(&'a mut Env) -> Pin<Box<dyn Future<Output = crate::semantics::Result> + 'a>>
        + 'static,
    // TODO Revisit to simplify this function type when impl Future is allowed in return type
{
    /// Creates a new subshell builder with a task.
    ///
    /// The task will run in a subshell after it is started.
    /// If the task returns an `Err(Divert::...)`, it is handled as follows:
    ///
    /// - `Interrupt` and `Exit` with `Some(exit_status)` override the exit
    ///   status in `Env`.
    /// - Other `Divert` values are ignored.
    pub fn new(task: F) -> Self {
        let job_control = None;
        Subshell { task, job_control }
    }

    /// Specifies disposition of the subshell with respect to job control.
    ///
    /// If the argument is `None`, the subshell runs in the same process group
    /// as the parent process. If it is `Some(_)`, the subshell becomes a new
    /// process group. For `JobControl::Foreground`, it also brings itself to
    /// the foreground.
    pub fn job_control<J: Into<Option<JobControl>>>(mut self, job_control: J) -> Self {
        self.job_control = job_control.into();
        self
    }

    /// Starts a subshell.
    ///
    /// This function creates a new child process that runs the task contained
    /// in this builder. If the child was started successfully, this function
    /// returns the child's process ID. Otherwise, it returns an error.
    ///
    /// Although this function is `async`, it does not wait for the child to
    /// finish, which means the parent and child processes will run
    /// concurrently. To wait for the child to finish, you need to call
    /// [`Env::wait_for_subshell`] or [`Env::wait_for_subshell_to_finish`]. If
    /// job control is active, you may want to add the process ID to `env.jobs`
    /// before waiting.
    pub async fn start(self, env: &mut Env) -> nix::Result<Pid> {
        // Do some preparation before starting a child process
        let tty = match self.job_control {
            None | Some(JobControl::Background) => None,
            // Open the tty in the parent process so we can reuse the FD for other jobs
            Some(JobControl::Foreground) => Some(env.get_tty()?),
        };

        // Define the child process task
        let me = Pid::from_raw(0);
        let task: ChildProcessTask = Box::new(move |env| {
            Box::pin(async move {
                let mut env = env.push_frame(Frame::Subshell);
                let env = &mut *env;

                if let Some(job_control) = self.job_control {
                    if let Ok(()) = env.system.setpgid(me, me) {
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

                env.traps.enter_subshell(&mut env.system);

                let result = (self.task)(env).await;
                env.apply_result(result);
            })
        });

        // Start the child
        let child = env.system.new_child_process()?;
        let child_pid = child(env, task).await;

        // The finishing
        if self.job_control.is_some() {
            // We should setpgid not only in the child but also in the parent to
            // make sure the child is in a new process group before the parent
            // returns from the start function.
            let _ = env.system.setpgid(child_pid, me);

            // We don't tcsetpgrp in the parent. It would mess up the child
            // which may have started another shell doing its own job control.
        }

        Ok(child_pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::INode;
    use crate::tests::in_virtual_system;
    use crate::trap::Action;
    use crate::trap::Signal;
    use assert_matches::assert_matches;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::ops::ControlFlow::Continue;
    use std::rc::Rc;
    use yash_syntax::source::Location;

    #[test]
    fn subshell_start_returns_child_process_id() {
        in_virtual_system(|mut env, parent_pid, _state| async move {
            let child_pid = Rc::new(Cell::new(None));
            let child_pid_2 = Rc::clone(&child_pid);
            let subshell = Subshell::new(move |env| {
                Box::pin(async move {
                    child_pid_2.set(Some(env.system.getpid()));
                    assert_eq!(env.system.getppid(), parent_pid);
                    Continue(())
                })
            });
            let result = subshell.start(&mut env).await.unwrap();
            env.wait_for_subshell(result).await.unwrap();
            assert_eq!(Some(result), child_pid.get());
        });
    }

    #[test]
    fn stack_frame_in_subshell() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let subshell = Subshell::new(|env| {
                Box::pin(async move {
                    assert_eq!(env.stack[..], [Frame::Subshell]);
                    Continue(())
                })
            });
            let pid = subshell.start(&mut env).await.unwrap();
            assert_eq!(env.stack[..], []);

            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn trap_reset_in_subshell() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.traps
                .set_action(
                    &mut env.system,
                    Signal::SIGCHLD,
                    Action::Command("echo foo".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
            let subshell = Subshell::new(|env| {
                Box::pin(async move {
                    let trap_state = assert_matches!(
                        env.traps.get_state(Signal::SIGCHLD),
                        (None, Some(trap_state)) => trap_state
                    );
                    assert_matches!(
                        &trap_state.action,
                        Action::Command(body) => assert_eq!(&**body, "echo foo")
                    );
                    Continue(())
                })
            });
            let pid = subshell.start(&mut env).await.unwrap();
            env.wait_for_subshell(pid).await.unwrap();
        });
    }

    #[test]
    fn subshell_with_no_job_control() {
        in_virtual_system(|mut parent_env, parent_pid, state| async move {
            let parent_pgid = state.borrow().processes[&parent_pid].pgid;
            let state_2 = Rc::clone(&state);
            let child_pid = Subshell::new(move |child_env| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, parent_pgid);
                    assert_eq!(state_2.borrow().foreground, None);
                    Continue(())
                })
            })
            .job_control(None)
            .start(&mut parent_env)
            .await
            .unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, parent_pgid);
            assert_eq!(state.borrow().foreground, None);
        });
    }

    #[test]
    fn subshell_in_background() {
        in_virtual_system(|mut parent_env, _pid, state| async move {
            let state_2 = Rc::clone(&state);
            let child_pid = Subshell::new(move |child_env| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                    assert_eq!(state_2.borrow().foreground, None);
                    Continue(())
                })
            })
            .job_control(JobControl::Background)
            .start(&mut parent_env)
            .await
            .unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
            assert_eq!(state.borrow().foreground, None);

            parent_env.wait_for_subshell(child_pid).await.unwrap();
            assert_eq!(state.borrow().processes[&child_pid].pgid, child_pid);
            assert_eq!(state.borrow().foreground, None);
        });
    }

    #[test]
    fn subshell_in_foreground() {
        in_virtual_system(|mut parent_env, _pid, state| async move {
            state
                .borrow_mut()
                .file_system
                .save("/dev/tty", Rc::new(RefCell::new(INode::new([]))))
                .unwrap();

            let state_2 = Rc::clone(&state);
            let child_pid = Subshell::new(move |child_env| {
                Box::pin(async move {
                    let child_pid = child_env.system.getpid();
                    assert_eq!(state_2.borrow().processes[&child_pid].pgid, child_pid);
                    assert_eq!(state_2.borrow().foreground, Some(child_pid));
                    Continue(())
                })
            })
            .job_control(JobControl::Foreground)
            .start(&mut parent_env)
            .await
            .unwrap();
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
        in_virtual_system(|mut parent_env, _pid, state| async move {
            state
                .borrow_mut()
                .file_system
                .save("/dev/tty", Rc::new(RefCell::new(INode::new([]))))
                .unwrap();

            let _ = Subshell::new(move |_env| Box::pin(async move { Continue(()) }))
                .job_control(JobControl::Foreground)
                .start(&mut parent_env)
                .await
                .unwrap();
            assert_matches!(parent_env.tty, Some(_));
        });
    }
}
