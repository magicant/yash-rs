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

//! Defines the [`EofGuard`] input decorator, [`SuspendedJobsGuardConfig`], and
//! [`IgnoreEofConfig`].

use super::{Context, Input, Result};
use crate::Env;
use crate::io::Fd;
use crate::option::{IgnoreEof as IgnoreEofOption, Interactive, Off, On};
use crate::system::Isatty;
use crate::system::concurrency::WriteAll;
use std::cell::RefCell;

/// Configuration for the suspended-jobs exit guard.
///
/// When present in [`Env::any`](crate::Env::any), [`EofGuard`] refuses to exit
/// when there are suspended jobs, printing [`message`](Self::message) to warn
/// the user. Other components may also opt in to this protection by checking
/// for this config.
///
/// Store this config in the environment with
/// `env.any.insert(Box::new(config))`.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct SuspendedJobsGuardConfig {
    /// Text displayed when exit is prevented because there are suspended jobs
    pub message: String,
}

impl SuspendedJobsGuardConfig {
    /// Creates a new `SuspendedJobsGuardConfig` with the given message.
    #[must_use]
    pub fn with_message<M: Into<String>>(message: M) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Configuration for the [`EofGuard`]'s `ignore-eof` behavior.
///
/// When present in [`Env::any`](crate::Env::any), [`EofGuard`] retries reading
/// on EOF when the [`ignore-eof` option](crate::option::IgnoreEof) is enabled,
/// printing [`message`](Self::message) to remind the user.
///
/// If absent from `env.any`, the `ignore-eof` EOF protection in [`EofGuard`]
/// is disabled.
///
/// Store this config in the environment with
/// `env.any.insert(Box::new(config))`.
///
/// Note that [`IgnoreEof`](crate::input::IgnoreEof) is not affected by this config.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct IgnoreEofConfig {
    /// Text displayed when EOF is ignored due to the `ignore-eof` option
    pub message: String,
}

impl IgnoreEofConfig {
    /// Creates a new `IgnoreEofConfig` with the given message.
    #[must_use]
    pub fn with_message<M: Into<String>>(message: M) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// `Input` decorator that prevents premature exit on EOF
///
/// This is a decorator of [`Input`] that combines the behavior of the
/// [`ignore-eof` shell option](crate::option::IgnoreEof) with protection
/// against accidental exit when there are suspended jobs.
///
/// On EOF (empty line), the decorator retries reading if any of the following
/// conditions is met (provided the shell is interactive, the input is a
/// terminal, and the retry limit has not been reached):
///
/// 1. There are suspended jobs and [`SuspendedJobsGuardConfig`] is present in
///    `env.any` — prints [`SuspendedJobsGuardConfig::message`].
/// 2. The `ignore-eof` option is enabled and [`IgnoreEofConfig`] is present in
///    `env.any` — prints [`IgnoreEofConfig::message`].
///
/// The retry limit is 50 consecutive EOFs per [`next_line`](Input::next_line)
/// call. Once the limit is reached the empty string is returned, allowing the
/// shell to exit.
///
/// If neither [`SuspendedJobsGuardConfig`] nor [`IgnoreEofConfig`] is present
/// in `env.any`, the decorator passes through all input unchanged (no retries).
///
/// Unlike [`IgnoreEof`](crate::input::IgnoreEof), this decorator also checks
/// for suspended jobs, so it should be used instead of `IgnoreEof` in the
/// top-level interactive read loop.
#[derive(Debug)]
pub struct EofGuard<'a, 'b, S, T> {
    /// Inner input to read from
    inner: T,
    /// File descriptor to be checked if it is a terminal
    fd: Fd,
    /// Environment to check the shell options, jobs, and system interface
    env: &'a RefCell<&'b mut Env<S>>,
}

impl<'a, 'b, S, T> EofGuard<'a, 'b, S, T> {
    /// Creates a new `EofGuard` decorator.
    ///
    /// The arguments match those of [`IgnoreEof::new`](crate::input::IgnoreEof::new)
    /// except that there is no `message` argument — the messages are read from
    /// [`SuspendedJobsGuardConfig`] and [`IgnoreEofConfig`] stored in `env.any`.
    ///
    /// `inner` is the wrapped input, `fd` is the terminal file descriptor to
    /// check, and `env` is the shared environment.
    pub fn new(inner: T, fd: Fd, env: &'a RefCell<&'b mut Env<S>>) -> Self {
        Self { inner, fd, env }
    }
}

// Not derived automatically because S may not implement Clone.
impl<S, T: Clone> Clone for EofGuard<'_, '_, S, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            fd: self.fd,
            env: self.env,
        }
    }
}

impl<S: Isatty + WriteAll, T: Input> Input for EofGuard<'_, '_, S, T> {
    #[allow(
        clippy::await_holding_refcell_ref,
        reason = "other decorators, the parser, or the executor do not run concurrently with this method"
    )]
    async fn next_line(&mut self, context: &Context) -> Result {
        let mut remaining_tries = 50;

        loop {
            let line = self.inner.next_line(context).await?;

            let env = self.env.borrow();

            if !line.is_empty()
                || env.options.get(Interactive) == Off
                || remaining_tries == 0
                || !env.system.isatty(self.fd)
            {
                return Ok(line);
            }

            // TODO: skip the suspended-jobs check in PosixlyCorrect mode
            let has_suspended = env.jobs.iter().any(|(_, job)| job.state.is_stopped());

            if has_suspended {
                let Some(config) = env.any.get::<SuspendedJobsGuardConfig>() else {
                    return Ok(line);
                };
                env.system.print_error(&config.message).await;
            } else if env.options.get(IgnoreEofOption) == On {
                let Some(config) = env.any.get::<IgnoreEofConfig>() else {
                    return Ok(line);
                };
                env.system.print_error(&config.message).await;
            } else {
                return Ok(line);
            }

            remaining_tries -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Memory;
    use super::*;
    use crate::job::{Job, Pid, ProcessState};
    use crate::option::On;
    use crate::system::r#virtual::{FdBody, FileBody, Inode, OpenFileDescription, VirtualSystem};
    use crate::system::{Concurrent, Mode};
    use crate::test_helper::assert_stderr;
    use enumset::EnumSet;
    use futures_util::FutureExt as _;
    use std::rc::Rc;

    fn set_stdin_to_tty(system: &mut VirtualSystem) {
        system
            .current_process_mut()
            .set_fd(
                Fd::STDIN,
                FdBody {
                    open_file_description: Rc::new(RefCell::new(OpenFileDescription::new(
                        Rc::new(RefCell::new(Inode {
                            body: FileBody::Terminal { content: vec![] },
                            permissions: Mode::empty(),
                        })),
                        /* offset = */ 0,
                        /* is_readable = */ true,
                        /* is_writable = */ true,
                        /* is_appending = */ false,
                        /* is_nonblocking = */ false,
                    ))),
                    flags: EnumSet::empty(),
                },
            )
            .unwrap();
    }

    fn set_stdin_to_regular_file(system: &mut VirtualSystem) {
        system
            .current_process_mut()
            .set_fd(
                Fd::STDIN,
                FdBody {
                    open_file_description: Rc::new(RefCell::new(OpenFileDescription::new(
                        Rc::new(RefCell::new(Inode {
                            body: FileBody::Regular {
                                content: vec![],
                                is_native_executable: false,
                            },
                            permissions: Mode::empty(),
                        })),
                        /* offset = */ 0,
                        /* is_readable = */ true,
                        /* is_writable = */ true,
                        /* is_appending = */ false,
                        /* is_nonblocking = */ false,
                    ))),
                    flags: EnumSet::empty(),
                },
            )
            .unwrap();
    }

    /// `Input` decorator that returns EOF for the first `count` calls
    /// and then reads from the inner input.
    struct EofStub<T> {
        inner: T,
        count: usize,
    }

    impl<T: Input> Input for EofStub<T> {
        async fn next_line(&mut self, context: &Context) -> Result {
            if let Some(remaining) = self.count.checked_sub(1) {
                self.count = remaining;
                Ok("".to_string())
            } else {
                self.inner.next_line(context).await
            }
        }
    }

    #[test]
    fn decorator_reads_from_inner_input() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        env.any.insert(Box::new(IgnoreEofConfig::default()));
        env.any
            .insert(Box::new(SuspendedJobsGuardConfig::default()));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(Memory::new("echo foo\n"), Fd::STDIN, &ref_env);

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo foo\n");
    }

    #[test]
    fn decorator_reads_input_again_on_eof_with_ignore_eof_option() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        env.any.insert(Box::new(IgnoreEofConfig {
            message: "EOF ignored\n".to_string(),
        }));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo foo\n");
        assert_stderr(&state, |stderr| assert_eq!(stderr, "EOF ignored\n"));
    }

    #[test]
    fn decorator_reads_input_up_to_50_times() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        env.any.insert(Box::new(IgnoreEofConfig {
            message: "EOF ignored\n".to_string(),
        }));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 50,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo foo\n");
        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "EOF ignored\n".repeat(50))
        });
    }

    #[test]
    fn decorator_returns_empty_line_after_reading_51_times() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        env.any.insert(Box::new(IgnoreEofConfig {
            message: "EOF ignored\n".to_string(),
        }));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 51,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "EOF ignored\n".repeat(50))
        });
    }

    #[test]
    fn decorator_returns_immediately_if_not_interactive() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        // Interactive is Off (default)
        env.options.set(IgnoreEofOption, On);
        env.any.insert(Box::new(IgnoreEofConfig::default()));
        env.any
            .insert(Box::new(SuspendedJobsGuardConfig::default()));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn decorator_returns_immediately_if_not_ignore_eof_and_no_suspended_jobs() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        // IgnoreEof is Off (default), no jobs
        env.any.insert(Box::new(IgnoreEofConfig::default()));
        env.any
            .insert(Box::new(SuspendedJobsGuardConfig::default()));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn decorator_returns_immediately_if_not_terminal() {
        let mut system = VirtualSystem::new();
        set_stdin_to_regular_file(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        env.any.insert(Box::new(IgnoreEofConfig::default()));
        env.any
            .insert(Box::new(SuspendedJobsGuardConfig::default()));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    // Counterpart to `decorator_reads_input_again_on_eof_with_ignore_eof_option`:
    // without `IgnoreEofConfig` in `env.any`, the ignore-eof retry is disabled,
    // so the decorator returns the EOF immediately.
    #[test]
    fn decorator_returns_immediately_if_no_ignore_eof_config() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        // No IgnoreEofConfig in env.any
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn decorator_ignores_eof_when_there_are_suspended_jobs() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        // IgnoreEof is Off, but there is a suspended job
        let mut job = Job::new(Pid(42));
        job.state = ProcessState::stopped(crate::system::r#virtual::SIGTSTP);
        env.jobs.insert(job);
        env.any.insert(Box::new(SuspendedJobsGuardConfig {
            message: "There are stopped jobs.\n".to_string(),
        }));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo foo\n");
        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "There are stopped jobs.\n")
        });
    }

    // Counterpart to `decorator_ignores_eof_when_there_are_suspended_jobs`:
    // without `SuspendedJobsGuardConfig` in `env.any`, the suspended-jobs guard
    // is disabled, so the decorator returns the EOF immediately.
    #[test]
    fn decorator_returns_immediately_if_no_suspended_jobs_config() {
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        // IgnoreEof is Off, but there is a suspended job
        let mut job = Job::new(Pid(42));
        job.state = ProcessState::stopped(crate::system::r#virtual::SIGTSTP);
        env.jobs.insert(job);
        // No SuspendedJobsGuardConfig in env.any
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn suspended_jobs_message_takes_priority_over_ignore_eof_message() {
        // The ignore-eof message typically tells the user to type `exit` to
        // leave the shell, but a plain `exit` does not work when there are
        // suspended jobs. The suspended-jobs message must take priority so the
        // user is correctly directed to use `exit -f` instead.
        let mut system = VirtualSystem::new();
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        let mut job = Job::new(Pid(42));
        job.state = ProcessState::stopped(crate::system::r#virtual::SIGTSTP);
        env.jobs.insert(job);
        env.any.insert(Box::new(IgnoreEofConfig {
            message: "EOF ignored\n".to_string(),
        }));
        env.any.insert(Box::new(SuspendedJobsGuardConfig {
            message: "There are stopped jobs.\n".to_string(),
        }));
        let ref_env = RefCell::new(&mut env);
        let mut decorator = EofGuard::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo foo\n");
        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "There are stopped jobs.\n")
        });
    }
}
