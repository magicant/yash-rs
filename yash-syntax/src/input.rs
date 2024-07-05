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

use async_trait::async_trait;
use std::ops::DerefMut;

/// Parameter passed to the input function
///
/// The context is passed to the [input function](Input::next_line) so that it
/// can read the input in a context-dependent way.
#[derive(Debug)]
#[non_exhaustive]
pub struct Context {
    pub(crate) is_first_line: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            is_first_line: true,
        }
    }
}

impl Context {
    /// Whether the current line is the first line of the input
    #[must_use]
    pub fn is_first_line(&self) -> bool {
        self.is_first_line
    }
}

/// Error returned by the [Input] function.
pub type Error = std::io::Error;

/// Result of the [Input] function.
pub type Result = std::result::Result<String, Error>;

/// Line-oriented source code reader.
///
/// An `Input` object provides the parser with source code by reading from underlying source.
#[async_trait(?Send)]
pub trait Input {
    /// Reads a next line of the source code.
    ///
    /// The input function is line-oriented; that is, this function returns a string that is
    /// terminated by a newline unless the end of input (EOF) is reached, in which case the
    /// remaining characters up to the EOF must be returned without a trailing newline. If there
    /// are no more characters at all, the returned line is empty.
    ///
    /// Errors returned from this function are considered unrecoverable. Once an error is returned,
    /// this function should not be called any more.
    ///
    /// For object safety, this async method is declared to return the future in a pinned box.
    async fn next_line(&mut self, context: &Context) -> Result;
}

#[async_trait(?Send)]
impl<T> Input for T
where
    T: DerefMut,
    T::Target: Input,
{
    async fn next_line(&mut self, context: &Context) -> Result {
        self.deref_mut().next_line(context).await
    }
}

/// Input function that reads from a string in memory.
pub struct Memory<'a> {
    lines: std::str::SplitInclusive<'a, char>,
}

impl Memory<'_> {
    /// Creates a new `Memory` that reads the given string.
    pub fn new(code: &str) -> Memory<'_> {
        let lines = code.split_inclusive('\n');
        Memory { lines }
    }
}

#[async_trait(?Send)]
impl Input for Memory<'_> {
    async fn next_line(&mut self, _context: &Context) -> Result {
        Ok(self.lines.next().unwrap_or("").to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    #[test]
    fn memory_empty_source() {
        let mut input = Memory::new("");
        let context = Context::default();

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "");
    }

    #[test]
    fn memory_one_line() {
        let mut input = Memory::new("one\n");
        let context = Context::default();

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "one\n");

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "");
    }

    #[test]
    fn memory_three_lines() {
        let mut input = Memory::new("one\ntwo\nthree");
        let context = Context::default();

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "one\n");

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "two\n");

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "three");

        let line = input.next_line(&context).now_or_never().unwrap().unwrap();
        assert_eq!(line, "");
    }
}
