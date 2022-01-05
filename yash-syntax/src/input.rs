// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

//! Methods about passing [source](crate::source) code to the [parser](crate::parser).

use crate::source::lines;
use crate::source::Code;
use crate::source::Lines;
use crate::source::Location;
use crate::source::Source;
use async_trait::async_trait;

/// Current state in which source code is read.
///
/// The context is passed to the input function so that it can read the input in a
/// context-dependent way.
///
/// Currently, this structure is empty. It may be extended to provide with some useful data in
/// future versions.
#[derive(Debug)]
pub struct Context;

/// Error returned by the [Input] function.
pub type Error = (Location, std::io::Error);

/// Result of the [Input] function.
pub type Result = std::result::Result<Code, Error>;

/// Line-oriented source code reader.
///
/// An `Input` object provides the parser with source code by reading from underlying source.
#[async_trait(?Send)]
pub trait Input {
    /// Reads a next line of the source code.
    ///
    /// The input function is line-oriented; that is, this function returns a [`Code`] that is
    /// terminated by a newline unless the end of input (EOF) is reached, in which case the
    /// remaining characters up to the EOF must be returned without a trailing newline. If there
    /// are no more characters at all, the returned line is empty.
    ///
    /// Errors returned from this function are considered unrecoverable. Once an error is returned,
    /// this function should not be called any more.
    ///
    /// Because the current Rust compiler does not support `async` functions in a trait, this
    /// function is explicitly declared to return a `Future` in a pinned box.
    async fn next_line(&mut self, context: &Context) -> Result;
}

/// Input function that reads from a string in memory.
pub struct Memory<'a> {
    lines: Lines<'a>,
}

impl Memory<'_> {
    /// Creates a new `Memory` that reads the given string.
    pub fn new(code: &str, source: Source) -> Memory<'_> {
        let lines = lines(code, source);
        Memory { lines }
    }

    fn next_line_sync(&mut self, _: &Context) -> Code {
        self.lines.next_or_empty()
    }
}

#[async_trait(?Send)]
impl Input for Memory<'_> {
    async fn next_line(&mut self, context: &Context) -> Result {
        Ok(self.next_line_sync(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures_executor::block_on;

    #[test]
    fn memory_empty_source() {
        let mut input = Memory::new("", Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn memory_one_line() {
        let mut input = Memory::new("one\n", Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "one\n");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.start_line_number.get(), 2);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn memory_three_lines() {
        let mut input = Memory::new("one\ntwo\nthree", Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "one\n");
        assert_eq!(line.start_line_number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "two\n");
        assert_eq!(line.start_line_number.get(), 2);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "three");
        assert_eq!(line.start_line_number.get(), 3);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.start_line_number.get(), 3);
        assert_eq!(line.source, Source::Unknown);
    }
}
