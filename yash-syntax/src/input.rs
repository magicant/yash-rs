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

use std::future::Future;
use std::ops::DerefMut;
use std::pin::Pin;

/// Parameter passed to the input function
///
/// The context is passed to the [input function](Input::next_line) so that it
/// can read the input in a context-dependent way.
#[derive(Debug)]
#[non_exhaustive]
pub struct Context {
    is_first_line: bool,
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
    #[inline]
    #[must_use]
    pub fn is_first_line(&self) -> bool {
        self.is_first_line
    }

    /// Sets whether the current line is the first line of the input
    ///
    /// This method is used by the lexer to set the flag. It can also be used in
    /// tests to simulate a non-first line. The default value is `true`.
    #[inline]
    pub fn set_is_first_line(&mut self, is_first_line: bool) {
        self.is_first_line = is_first_line;
    }
}

/// Error returned by the [Input] function.
pub type Error = std::io::Error;

/// Result of the [Input] function.
pub type Result = std::result::Result<String, Error>;

/// Line-oriented source code reader
///
/// An `Input` implementor provides the parser with source code by reading from underlying source.
///
/// [`InputObject`] is an object-safe version of this trait.
#[must_use = "Input instances should be used by a parser"]
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
    fn next_line(&mut self, context: &Context) -> impl Future<Output = Result>;
}

impl<T> Input for T
where
    T: DerefMut,
    T::Target: Input,
{
    fn next_line(&mut self, context: &Context) -> impl Future<Output = Result> {
        self.deref_mut().next_line(context)
    }
}

/// Object-safe adapter for the [`Input`] trait
///
/// `InputObject` is an object-safe version of the [`Input`] trait. It allows
/// the trait to be used as a trait object, which is necessary for dynamic
/// dispatch.
///
/// The umbrella implementation is provided for all types that implement the
/// [`Input`] trait.
pub trait InputObject {
    fn next_line<'a>(
        &'a mut self,
        context: &'a Context,
    ) -> Pin<Box<dyn Future<Output = Result> + 'a>>;
}

impl<T: Input> InputObject for T {
    fn next_line<'a>(
        &'a mut self,
        context: &'a Context,
    ) -> Pin<Box<dyn Future<Output = Result> + 'a>> {
        Box::pin(Input::next_line(self, context))
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

impl<'a> From<&'a str> for Memory<'a> {
    fn from(code: &'a str) -> Memory<'a> {
        Memory::new(code)
    }
}

impl Input for Memory<'_> {
    async fn next_line(&mut self, _context: &Context) -> Result {
        Ok(self.lines.next().unwrap_or("").to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, Input, Memory};
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
