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

use crate::source::Line;
use std::future::Future;
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
    ) -> Pin<Box<dyn Future<Output = Result<Line, std::io::Error>>>>;
}
