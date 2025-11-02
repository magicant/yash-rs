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

//! Types for using the shell language parser

use crate::input::InputObject;
use crate::source::Source;
use derive_more::Debug;
use std::num::NonZeroU64;
use std::rc::Rc;
use yash_syntax::parser::lex::Lexer;

/// Configuration for the parser
///
/// This struct holds various configuration options for the parser, including
/// the input function to read source code and source information.
///
/// Parser implementations are not provided in this crate (`yash-env`). The
/// standard parser implementation is provided in the `yash-syntax` crate.
/// `Config` is provided here so that other crates can use [`RunReadEvalLoop`]
/// without depending on `yash-syntax`.
///
/// [`RunReadEvalLoop`]: crate::semantics::RunReadEvalLoop
#[derive(Debug)]
#[non_exhaustive]
pub struct Config<'a> {
    /// Input function to read source code
    #[debug(skip)]
    pub input: Box<dyn InputObject + 'a>,

    /// Line number for the first line of the input
    ///
    /// The lexer counts lines starting from this number. This affects the
    /// `start_line_number` field of the [`Code`] instance attached to the
    /// parsed AST.
    ///
    /// The default value is `1`.
    ///
    /// [`Code`]: crate::source::Code
    pub start_line_number: NonZeroU64,

    /// Source information for the input
    ///
    /// If provided, this source information is saved in the `source` field of
    /// the [`Code`] instance attached to the parsed AST.
    ///
    /// The default value is `None`, in which case `Source::Unknown` is used.
    ///
    /// [`Code`]: crate::source::Code
    pub source: Option<Rc<Source>>,
}

impl<'a> Config<'a> {
    /// Creates a `Config` with the given input function.
    #[must_use]
    pub fn with_input(input: Box<dyn InputObject + 'a>) -> Self {
        Self {
            input,
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: None,
        }
    }

    /// Creates a lexer using this configuration.
    ///
    /// **Breaking change notice**: This method is provided only temporarily to
    /// ease the migration to the new parser API. It will be removed soon.
    pub fn into_lexer(self) -> Lexer<'a> {
        let mut config = yash_syntax::parser::lex::Config::new();
        config.start_line_number = self.start_line_number;
        config.source = self.source;
        config.input(self.input)
    }
}
