// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Reading input

use thiserror::Error;
use yash_env::system::Errno;
use yash_env::Env;
use yash_semantics::expansion::attr::AttrChar;
use yash_syntax::source::pretty::{AnnotationType, Message};

/// Error reading from the standard input
///
/// This error is returned by [`read`] when an error occurs while reading from
/// the standard input.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("error reading from the standard input: {errno}")]
pub struct Error {
    pub errno: Errno,
}

impl Error {
    /// Converts this error to a message.
    #[must_use]
    pub fn to_message(&self) -> Message {
        Message {
            r#type: AnnotationType::Error,
            title: self.to_string().into(),
            annotations: vec![],
        }
    }
}

impl<'a> From<&'a Error> for Message<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_message()
    }
}

/// Reads a line from the standard input.
///
/// This function reads a line from the standard input and returns a vector of
/// [`AttrChar`]s representing the line. The line is terminated by a newline
/// character, which is not included in the returned vector.
///
/// If `is_raw` is `true`, the read line is not subject to backslash processing.
/// Otherwise, backslash-newline pairs are treated as line continuations, and
/// other backslashes are treated as quoting characters. On encountering a line
/// continuation, this function removes the backslash-newline pair and continues
/// reading the next line.
pub async fn read(env: &mut Env, is_raw: bool) -> Result<Vec<AttrChar>, Error> {
    _ = env;
    _ = is_raw;
    todo!("read")
    // - Read one byte
    // - Convert to UTF-8, possibly leaving a partial character
    // - Annotate the characters
    // - Exit loop
}
