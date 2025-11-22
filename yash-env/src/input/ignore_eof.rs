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

//! Defines the [`IgnoreEof`] input decorator.

use super::{Context, Input, Result};
use crate::Env;
use crate::io::Fd;
use crate::option::{IgnoreEof as IgnoreEofOption, Interactive, Off};
use crate::system::System as _;
use std::cell::RefCell;

/// `Input` decorator that ignores EOF on a terminal
///
/// This is a decorator of [`Input`] that adds the behavior of the
/// [`ignore-eof` shell option](crate::option::IgnoreEof).
///
/// The decorator is effective only when all of the following conditions are
/// met:
///
/// - The shell is interactive, that is, the [`Interactive`] option is enabled.
/// - The `ignore-eof` option is enabled.
/// - The input is a terminal.
///
/// The decorator reads from the inner input and usually returns the result
/// as is. However, if the result is an empty string and the above conditions
/// are met, the decorator will re-read the input until a non-empty string
/// is obtained, an error occurs, or this process is repeated 20 times.
///
/// [`Interactive`]: crate::option::Interactive
#[derive(Clone, Debug)]
pub struct IgnoreEof<'a, 'b, T> {
    /// Inner input to read from
    inner: T,
    /// File descriptor to be checked if it is a terminal
    fd: Fd,
    /// Environment to check the shell options and interact with the system
    env: &'a RefCell<&'b mut Env>,
    /// Text to be displayed when EOF is ignored
    message: String,
}

impl<'a, 'b, T> IgnoreEof<'a, 'b, T> {
    /// Creates a new `IgnoreEof` decorator.
    ///
    /// The first argument is the inner `Input` that performs the actual input
    /// operation. The second argument is the file descriptor to be checked if
    /// it is a terminal. The third argument is the shell environment that
    /// contains the shell option state and the system interface to interact
    /// with the system.  It is wrapped in a `RefCell` so that it can be shared
    /// with other decorators and the parser. The fourth argument is the text to
    /// be displayed when EOF is ignored.
    ///
    /// The second argument `fd` should match the file descriptor that the inner
    /// input reads from. If the inner input reads from a different file
    /// descriptor, the `IgnoreEof` decorator may not detect the terminal
    /// correctly.
    pub fn new(inner: T, fd: Fd, env: &'a RefCell<&'b mut Env>, message: String) -> Self {
        Self {
            inner,
            fd,
            env,
            message,
        }
    }
}

impl<T> Input for IgnoreEof<'_, '_, T>
where
    T: Input,
{
    #[allow(clippy::await_holding_refcell_ref)]
    async fn next_line(&mut self, context: &Context) -> Result {
        let mut remaining_tries = 50;

        loop {
            let line = self.inner.next_line(context).await?;

            let env = self.env.borrow();

            let should_break = !line.is_empty()
                || env.options.get(Interactive) == Off
                || env.options.get(IgnoreEofOption) == Off
                || remaining_tries == 0
                || !env.system.isatty(self.fd);
            if should_break {
                return Ok(line);
            }

            env.system.print_error(&self.message).await;
            remaining_tries -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Memory;
    use super::*;
    use crate::option::On;
    use crate::system::Mode;
    use crate::system::r#virtual::{FdBody, FileBody, Inode, OpenFileDescription, VirtualSystem};
    use crate::tests::assert_stderr;
    use enumset::EnumSet;
    use futures_util::FutureExt as _;
    use std::rc::Rc;

    /// `Input` decorator that returns EOF for the first `count` calls
    /// and then reads from the inner input.
    struct EofStub<T> {
        inner: T,
        count: usize,
    }

    impl<T> Input for EofStub<T>
    where
        T: Input,
    {
        async fn next_line(&mut self, context: &Context) -> Result {
            if let Some(remaining) = self.count.checked_sub(1) {
                self.count = remaining;
                Ok("".to_string())
            } else {
                self.inner.next_line(context).await
            }
        }
    }

    fn set_stdin_to_tty(system: &mut VirtualSystem) {
        system
            .current_process_mut()
            .set_fd(
                Fd::STDIN,
                FdBody {
                    open_file_description: Rc::new(RefCell::new(OpenFileDescription {
                        file: Rc::new(RefCell::new(Inode {
                            body: FileBody::Terminal { content: vec![] },
                            permissions: Mode::empty(),
                        })),
                        offset: 0,
                        is_readable: true,
                        is_writable: true,
                        is_appending: false,
                    })),
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
                    open_file_description: Rc::new(RefCell::new(OpenFileDescription {
                        file: Rc::new(RefCell::new(Inode {
                            body: FileBody::Regular {
                                content: vec![],
                                is_native_executable: false,
                            },
                            permissions: Mode::empty(),
                        })),
                        offset: 0,
                        is_readable: true,
                        is_writable: true,
                        is_appending: false,
                    })),
                    flags: EnumSet::empty(),
                },
            )
            .unwrap();
    }

    #[test]
    fn decorator_reads_from_inner_input() {
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_tty(&mut system);
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            Memory::new("echo foo\n"),
            Fd::STDIN,
            &ref_env,
            "unused".to_string(),
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo foo\n");
    }

    #[test]
    fn decorator_reads_input_again_on_eof() {
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
            "EOF ignored\n".to_string(),
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
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 50,
            },
            Fd::STDIN,
            &ref_env,
            "EOF ignored\n".to_string(),
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
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 51,
            },
            Fd::STDIN,
            &ref_env,
            "EOF ignored\n".to_string(),
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
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.options.set(Interactive, Off);
        env.options.set(IgnoreEofOption, On);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
            "EOF ignored\n".to_string(),
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn decorator_returns_immediately_if_not_ignore_eof() {
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_tty(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, Off);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
            "EOF ignored\n".to_string(),
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
        let mut system = Box::new(VirtualSystem::new());
        set_stdin_to_regular_file(&mut system);
        let state = system.state.clone();
        let mut env = Env::with_system(system);
        env.options.set(Interactive, On);
        env.options.set(IgnoreEofOption, On);
        let ref_env = RefCell::new(&mut env);
        let mut decorator = IgnoreEof::new(
            EofStub {
                inner: Memory::new("echo foo\n"),
                count: 1,
            },
            Fd::STDIN,
            &ref_env,
            "EOF ignored\n".to_string(),
        );

        let result = decorator
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }
}
