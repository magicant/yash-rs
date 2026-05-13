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

// `FdReader2` definition

use super::{Context, Input, Result};
use crate::io::Fd;
use crate::system::Read;
use std::slice::from_mut;

/// [Input function](Input) that reads from a file descriptor
///
/// An instance of `FdReader2<S>` contains a system of type `S` that is used to
/// read from the file descriptor. The system must implement the `Read` trait to
/// allow the reading operation.
///
/// Although `FdReader2` implements `Clone`, cloning an instance does not allow
/// you to replay the input later. If the system is contained in `Rc`, for
/// example, reading from one instance will affect subsequent reads from the
/// other, since they share the same system and file descriptor.
///
/// This struct is named `FdReader2` to distinguish it from `FdReader`, an older
/// implementation that existed before the `Concurrent` system was implemented.
/// The `FdReader` struct has been removed, but the name `FdReader2` is kept for
/// backward compatibility.
#[derive(Clone, Debug)]
#[must_use = "FdReader2 does nothing unless used by a parser"]
pub struct FdReader2<S> {
    /// File descriptor to read from
    fd: Fd,
    /// System to interact with the FD
    system: S,
}

impl<S> FdReader2<S> {
    /// Creates a new `FdReader2` instance.
    ///
    /// The `fd` argument is the file descriptor to read from. It should be
    /// readable, have the close-on-exec flag set, and remain open for the
    /// lifetime of the `FdReader2` instance.
    pub fn new(fd: Fd, system: S) -> Self {
        FdReader2 { fd, system }
    }
}

impl<S: Read> Input for FdReader2<S> {
    async fn next_line(&mut self, _context: &Context) -> Result {
        // TODO Read many bytes at once if seekable

        let mut bytes = Vec::new();
        loop {
            let mut byte = 0;
            match self.system.read(self.fd, from_mut(&mut byte)).await {
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

        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::Concurrent;
    use crate::system::Errno;
    use crate::system::Mode;
    use crate::system::OfdAccess;
    use crate::system::Open as _;
    use crate::system::OpenFlag;
    use crate::system::r#virtual::FileBody;
    use crate::system::r#virtual::Inode;
    use crate::system::r#virtual::VirtualSystem;
    use futures_util::FutureExt as _;
    use std::rc::Rc;

    #[test]
    fn empty_reader() {
        let system = VirtualSystem::new();
        let system = Rc::new(Concurrent::new(system));
        let mut reader = FdReader2::new(Fd::STDIN, system);

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
        let system = Rc::new(Concurrent::new(system));
        let mut reader = FdReader2::new(Fd::STDIN, system);

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
        let system = Rc::new(Concurrent::new(system));
        let mut reader = FdReader2::new(Fd::STDIN, system);

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
            let file = Rc::new(Inode::new("echo file\n").into());
            state.file_system.save("/foo", file).unwrap();
        }
        let system = Rc::new(Concurrent::new(system));
        let path = c"/foo";
        let fd = system
            .open(
                path,
                OfdAccess::ReadOnly,
                OpenFlag::CloseOnExec.into(),
                Mode::empty(),
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut reader = FdReader2::new(fd, system);

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
        let system = VirtualSystem::new();
        system.current_process_mut().close_fd(Fd::STDIN);
        let system = Rc::new(Concurrent::new(system));
        let mut reader = FdReader2::new(Fd::STDIN, system);

        let error = reader
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(Errno::EBADF.0));
    }
}
