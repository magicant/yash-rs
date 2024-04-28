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

//! Methods about passing [source](yash_syntax::source) code to the
//! [parser](yash_syntax::parser).
//!
//! This module extends [`yash_syntax::input`] with input functions that are
//! implemented depending on the environment.

use crate::io::Fd;
use crate::option::State;
use crate::system::SharedSystem;
use async_trait::async_trait;
use std::cell::Cell;
use std::rc::Rc;
use std::slice::from_mut;

#[doc(no_inline)]
pub use yash_syntax::input::*;

/// Input function that reads from a file descriptor.
///
/// An instance of `FdReader` contains a [`SharedSystem`] to interact with the
/// file descriptor.
///
/// Although `FdReader` implements `Clone`, it does not mean you can create and
/// keep a copy of a `FdReader` instance to replay the input later. Since both
/// the original and clone share the same `SharedSystem`, reading a line from
/// one instance will affect the next read from the other.
#[derive(Clone, Debug)]
pub struct FdReader {
    /// File descriptor to read from
    fd: Fd,
    /// System to interact with the FD
    system: SharedSystem,
    /// Whether lines read are echoed to stderr
    echo: Option<Rc<Cell<State>>>,
}

impl FdReader {
    /// Creates a new `FdReader` instance.
    ///
    /// The `fd` argument is the file descriptor to read from. It should be
    /// readable, have the close-on-exec flag set, and remain open for the
    /// lifetime of the `FdReader` instance.
    pub fn new(fd: Fd, system: SharedSystem) -> Self {
        let echo = None;
        FdReader { fd, system, echo }
    }

    /// Sets the "echo" flag.
    ///
    /// You can use this setter function to set a shared option state that
    /// controls whether the input function echoes lines it reads to the
    /// standard error. If `echo` is `None` or some shared cell containing
    /// `Off`, the function does not echo. If a cell has `On`, the function
    /// prints every line it reads to the standard error.
    ///
    /// This option implements the behavior of the `verbose` shell option. You
    /// can change the state of the shared cell through the lifetime of the
    /// input function to reflect the option dynamically changed, which will
    /// affect the next `next_line` call.
    pub fn set_echo(&mut self, echo: Option<Rc<Cell<State>>>) {
        self.echo = echo;
    }
}

#[async_trait(?Send)]
impl Input for FdReader {
    async fn next_line(&mut self, _context: &Context) -> Result {
        // TODO Read many bytes at once if seekable

        let mut bytes = Vec::new();
        loop {
            let mut byte = 0;
            match self.system.read_async(self.fd, from_mut(&mut byte)).await {
                // End of input
                Ok(0) => break,

                Ok(count) => {
                    assert_eq!(count, 1);
                    bytes.push(byte);
                    if byte == b'\n' {
                        break;
                    }
                }

                Err(errno) => return Err(errno.into()),
            }
        }

        // TODO Reject invalid UTF-8 sequence if strict POSIX mode is on
        let line = String::from_utf8(bytes)
            .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into());

        if let Some(echo) = &self.echo {
            if echo.get() == State::On {
                let _ = self.system.write_all(Fd::STDERR, line.as_bytes()).await;
            }
        }

        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::FileBody;
    use crate::system::r#virtual::INode;
    use crate::system::r#virtual::VirtualSystem;
    use crate::system::Errno;
    use crate::system::Mode;
    use crate::system::OFlag;
    use crate::System;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::ffi::CStr;

    #[test]
    fn empty_reader() {
        let system = VirtualSystem::new();
        let system = SharedSystem::new(Box::new(system));
        let mut reader = FdReader::new(Fd::STDIN, system);

        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "");
    }

    #[test]
    fn one_line_reader() {
        let system = VirtualSystem::new();
        {
            let state = system.state.borrow_mut();
            let file = state.file_system.get("/dev/stdin").unwrap();
            file.borrow_mut().body = FileBody::new(*b"echo ok\n");
        }
        let system = SharedSystem::new(Box::new(system));
        let mut reader = FdReader::new(Fd::STDIN, system);

        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "echo ok\n");
        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "");
    }

    #[test]
    fn reader_with_many_lines() {
        let system = VirtualSystem::new();
        {
            let state = system.state.borrow_mut();
            let file = state.file_system.get("/dev/stdin").unwrap();
            file.borrow_mut().body = FileBody::new(*b"#!/bin/sh\necho ok\nexit");
        }
        let system = SharedSystem::new(Box::new(system));
        let mut reader = FdReader::new(Fd::STDIN, system);

        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "#!/bin/sh\n");
        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "echo ok\n");
        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "exit");
        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "");
    }

    #[test]
    fn reading_from_file() {
        let system = VirtualSystem::new();
        {
            let mut state = system.state.borrow_mut();
            let file = Rc::new(INode::new("echo file\n").into());
            state.file_system.save("/foo", file).unwrap();
        }
        let mut system = SharedSystem::new(Box::new(system));
        let path = CStr::from_bytes_with_nul(b"/foo\0").unwrap();
        let fd = system.open(path, OFlag::O_RDONLY, Mode::empty()).unwrap();
        let mut reader = FdReader::new(fd, system);

        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "echo file\n");
        let line = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "");
    }

    #[test]
    fn reader_error() {
        let mut system = VirtualSystem::new();
        system.current_process_mut().close_fd(Fd::STDIN);
        let system = SharedSystem::new(Box::new(system));
        let mut reader = FdReader::new(Fd::STDIN, system);

        let error = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(Errno::EBADF.0));
    }

    #[test]
    fn echo_off() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        {
            let state = state.borrow();
            let file = state.file_system.get("/dev/stdin").unwrap();
            file.borrow_mut().body = FileBody::new(*b"one\ntwo");
        }
        let system = SharedSystem::new(Box::new(system));
        let mut reader = FdReader::new(Fd::STDIN, system);
        reader.set_echo(Some(Rc::new(Cell::new(State::Off))));

        let _ = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        let state = state.borrow();
        let file = state.file_system.get("/dev/stderr").unwrap();
        assert_matches!(&file.borrow().body, FileBody::Regular { content, .. } => {
            assert_eq!(content, &[]);
        });
    }

    #[test]
    fn echo_on() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        {
            let state = state.borrow();
            let file = state.file_system.get("/dev/stdin").unwrap();
            file.borrow_mut().body = FileBody::new(*b"one\ntwo");
        }
        let system = SharedSystem::new(Box::new(system));
        let mut reader = FdReader::new(Fd::STDIN, system);
        reader.set_echo(Some(Rc::new(Cell::new(State::On))));

        let _ = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        {
            let state = state.borrow();
            let file = state.file_system.get("/dev/stderr").unwrap();
            assert_matches!(&file.borrow().body, FileBody::Regular { content, .. } => {
                assert_eq!(content, b"one\n");
            });
        }
        let _ = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        {
            let state = state.borrow();
            let file = state.file_system.get("/dev/stderr").unwrap();
            assert_matches!(&file.borrow().body, FileBody::Regular { content, .. } => {
                assert_eq!(content, b"one\ntwo");
            });
        }
    }
}
