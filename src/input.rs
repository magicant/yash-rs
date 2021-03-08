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
use crate::source::Line;
use crate::source::Location;
use crate::source::Source;
use std::collections::VecDeque;
use std::future::ready;
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::Pin;

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

/// Line-oriented source code reader.
///
/// An `Input` object provides the parser with source code by reading from underlying source.
pub trait Input {
    /// Reads a next line of the source code.
    ///
    /// The input function is line-oriented; that is, this function returns a [`Line`] that is
    /// terminated by a newline unless the end of input (EOF) is reached, in which case the
    /// remaining characters up to the EOF must be returned without a trailing newline. If there
    /// are no more characters at all, the returned line is empty.
    ///
    /// Errors returned from this function are considered unrecoverable. Once an error is returned,
    /// this function should not be called any more.
    ///
    /// Because the current Rust compiler does not support `async` functions in a trait, this
    /// function is explicitly declared to return a `Future` in a pinned box.
    fn next_line(
        &mut self,
        context: &Context,
    ) -> Pin<Box<dyn Future<Output = Result<Line, Error>>>>;
}

/// Input function that reads from a string in memory.
pub struct Memory {
    lines: VecDeque<Line>,
    end: Line,
}

impl Memory {
    /// Creates a new `Memory` that reads the given string.
    pub fn new(source: Source, code: &str) -> Memory {
        let lines = lines(source.clone(), code).collect::<VecDeque<Line>>();

        let end = Line {
            value: "".to_string(),
            number: if let Some(last_line) = lines.back() {
                // TODO Not correct if the last line does not end with a newline
                NonZeroU64::new(last_line.number.get() + 1).expect("too long source code line")
            } else {
                NonZeroU64::new(1).unwrap()
            },
            source,
        };

        Memory { lines, end }
    }

    fn next_line_sync(&mut self, _: &Context) -> Line {
        self.lines.pop_front().unwrap_or_else(|| self.end.clone())
    }
}

impl Input for Memory {
    fn next_line(
        &mut self,
        context: &Context,
    ) -> Pin<Box<dyn Future<Output = Result<Line, Error>>>> {
        Box::pin(ready(Ok(self.next_line_sync(context))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

    #[test]
    fn memory_empty_source() {
        let mut input = Memory::new(Source::Unknown, "");

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn memory_one_line() {
        let mut input = Memory::new(Source::Unknown, "one\n");

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "one\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Unknown);
    }

    #[test]
    fn memory_three_lines() {
        let mut input = Memory::new(Source::Unknown, "one\ntwo\nthree");

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "one\n");
        assert_eq!(line.number.get(), 1);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "two\n");
        assert_eq!(line.number.get(), 2);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "three");
        assert_eq!(line.number.get(), 3);
        assert_eq!(line.source, Source::Unknown);

        let line = block_on(input.next_line(&Context)).unwrap();
        assert_eq!(line.value, "");
        assert_eq!(line.number.get(), 4);
        assert_eq!(line.source, Source::Unknown);
    }
}
