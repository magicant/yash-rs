// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! `Memory` definition

use super::{Context, Input, Result};

/// Input function that reads from a string in memory
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
    use futures_util::FutureExt as _;

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
