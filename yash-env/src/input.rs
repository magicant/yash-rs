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
use crate::system::SharedSystem;
use async_trait::async_trait;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::slice::from_mut;
use yash_syntax::source::Code;
use yash_syntax::source::Location;
use yash_syntax::source::Source;

#[doc(no_inline)]
pub use yash_syntax::input::*;

/// Input function that reads from the standard input.
///
/// An instance of `Stdin` contains a [`SharedSystem`] to read the input from,
/// as well as the current line number.
///
/// Although `Stdin` implements `Clone`, it does not mean you can create and
/// keep a copy of a `Stdin` instance to replay the input later. Since both the
/// original and clone share the same `SharedSystem`, reading a line from one
/// instance will affect the next read from the other instance.
#[derive(Clone, Debug)]
pub struct Stdin {
    system: SharedSystem,
    line_number: NonZeroU64,
}

impl Stdin {
    /// Creates a new `Stdin` instance.
    pub fn new(system: SharedSystem) -> Self {
        Stdin {
            system,
            line_number: NonZeroU64::new(1).unwrap(),
        }
    }

    /// Returns the current line number.
    pub fn line_number(&self) -> NonZeroU64 {
        self.line_number
    }

    /// Overwrites the current line number.
    pub fn set_line_number(&mut self, line_number: NonZeroU64) {
        self.line_number = line_number;
    }
}

#[async_trait(?Send)]
impl Input for Stdin {
    async fn next_line(&mut self, _context: &Context) -> Result {
        // TODO Read many bytes at once if seekable

        fn to_code(bytes: Vec<u8>, number: NonZeroU64) -> Code {
            // TODO Maybe we should report invalid UTF-8 bytes rather than ignoring them
            let value = String::from_utf8(bytes)
                .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).to_string());
            Code {
                value,
                number,
                source: Source::Stdin,
            }
        }

        let number = self.line_number;
        let mut bytes = Vec::new();
        loop {
            let mut byte = 0;
            match self.system.read_async(Fd::STDIN, from_mut(&mut byte)).await {
                // End of input
                Ok(0) => break,

                Ok(count) => {
                    assert_eq!(count, 1);
                    bytes.push(byte);
                    if byte == b'\n' {
                        // TODO self.line_number = self.line_number.saturating_add(1);
                        self.line_number = unsafe {
                            NonZeroU64::new_unchecked(self.line_number.get().saturating_add(1))
                        };
                        break;
                    }
                }

                Err(errno) => {
                    let code = Rc::new(to_code(bytes, number));
                    let column = code
                        .value
                        .chars()
                        .count()
                        .try_into()
                        .unwrap_or(u64::MAX)
                        .saturating_add(1);
                    let column = unsafe { NonZeroU64::new_unchecked(column) };
                    let location = Location { code, column };
                    let error = std::io::Error::from_raw_os_error(errno as i32);
                    return Err((location, error));
                }
            }
        }

        Ok(to_code(bytes, number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::VirtualSystem;
    use crate::system::Errno;
    use futures_executor::block_on;

    #[test]
    fn stdin_empty() {
        let system = VirtualSystem::new();
        let system = SharedSystem::new(Box::new(system));
        let mut stdin = Stdin::new(system);

        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Stdin);
    }

    #[test]
    fn stdin_one_line() {
        let system = VirtualSystem::new();
        {
            let state = system.state.borrow_mut();
            let mut file = state.file_system.get("/dev/stdin").unwrap().borrow_mut();
            file.content.extend("echo ok\n".as_bytes());
        }
        let system = SharedSystem::new(Box::new(system));
        let mut stdin = Stdin::new(system);

        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "echo ok\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Stdin);
        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Stdin);
    }

    #[test]
    fn stdin_many_lines() {
        let system = VirtualSystem::new();
        {
            let state = system.state.borrow_mut();
            let mut file = state.file_system.get("/dev/stdin").unwrap().borrow_mut();
            file.content.extend("#!/bin/sh\necho ok\nexit".as_bytes());
        }
        let system = SharedSystem::new(Box::new(system));
        let mut stdin = Stdin::new(system);

        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "#!/bin/sh\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Stdin);
        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "echo ok\n");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Stdin);
        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "exit");
        assert_eq!(line.number.get(), 3);
        assert_eq!(line.source, Source::Stdin);
        let line = block_on(stdin.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.number.get(), 3);
        assert_eq!(line.source, Source::Stdin);
    }

    #[test]
    fn stdin_error() {
        let mut system = VirtualSystem::new();
        system.current_process_mut().close_fd(Fd::STDIN);
        let system = SharedSystem::new(Box::new(system));
        let mut stdin = Stdin::new(system);

        let (location, error) = block_on(stdin.next_line(&Context)).unwrap_err();
        assert_eq!(location.code.value, "");
        assert_eq!(location.code.number.get(), 1);
        assert_eq!(location.code.source, Source::Stdin);
        assert_eq!(error.raw_os_error(), Some(Errno::EBADF as i32));
    }
}
