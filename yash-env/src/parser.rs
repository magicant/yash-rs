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

//! Shell language parser configuration and utilities
//!
//! This module contains several items related to the shell language parser.
//!
//! - [`Config`] is a struct that holds configuration options for the parser.
//! - [`IsKeyword`] is a wrapper for a function that checks if a string is a
//!   reserved word.
//! - [`IsName`] is a wrapper for a function that checks if a string is a valid
//!   variable name.
//!
//! Parser implementations are not provided in this crate (`yash-env`). The
//! standard parser implementation is provided in the `yash-syntax` crate.

use crate::Env;
use crate::input::InputObject;
use crate::option::Option::Portable;
use crate::option::OptionSet;
use crate::source::Source;
use derive_more::Debug;
use std::num::NonZeroU64;
use std::rc::Rc;

/// Parsing mode derived from shell options
///
/// Some shell options change which syntax the parser accepts. This type conveys
/// the relevant option states from the shell environment to the parser and
/// lexer, so that the parser does not need to depend on the whole
/// [`OptionSet`]. The standard parser ([`yash-syntax`](https://crates.io/crates/yash-syntax))
/// reads it from the [lexer](crate::parser) and adjusts the syntax it accepts
/// accordingly.
///
/// A `Mode` is typically created from the current option set by converting an
/// [`OptionSet`] with the [`From`] implementation. The [default](Default) mode
/// permits all syntax (every field is `false`), so the parser behaves as if no
/// restricting option were set.
///
/// This struct is `#[non_exhaustive]` because more fields may be added as more
/// parsing-affecting options are supported. You can still create one by starting
/// from [`Default`] or a converted [`OptionSet`] and assigning to individual
/// fields.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Mode {
    /// Whether the parser rejects non-portable syntax
    ///
    /// This reflects the [`Portable`] shell option. When `true`, the parser
    /// reports an error on constructs that are valid in yash but not portable
    /// across POSIX-conforming shells.
    pub portable: bool,
}

/// Creates a `Mode` reflecting the given option set.
impl From<&OptionSet> for Mode {
    fn from(options: &OptionSet) -> Self {
        Mode {
            portable: options.get(Portable).into(),
        }
    }
}

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
/// Since this struct is marked as `#[non_exhaustive]`, you cannot construct it
/// directly. Instead, use the [`with_input`](Self::with_input) function to
/// create a `Config` instance, and then modify its fields as necessary.
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

    /// Parsing mode derived from shell options
    ///
    /// The parser uses this to decide which syntax to accept. The default value
    /// is [`Mode::default`], which permits all syntax.
    pub mode: Mode,
}

impl<'a> Config<'a> {
    /// Creates a `Config` with the given input function.
    #[must_use]
    pub fn with_input(input: Box<dyn InputObject + 'a>) -> Self {
        Self {
            input,
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: None,
            mode: Mode::default(),
        }
    }
}

/// Wrapper for a function that checks if a string is a keyword
///
/// This struct wraps a function that takes an environment and a string, and
/// returns `true` if the string is a shell reserved word (keyword) in the given
/// environment. An implementation of the function should be provided and stored
/// in the environment's [`any`](Env::any) storage. This allows modules that
/// need to check for keywords to do so without directly depending on the parser
/// crate (`yash-syntax`).
#[derive(Debug)]
pub struct IsKeyword<S>(pub fn(&Env<S>, &str) -> bool);

// Not derived automatically because S may not implement Clone or Copy.
impl<S> Clone for IsKeyword<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for IsKeyword<S> {}

/// Wrapper for a function that checks if a string is a valid variable name
///
/// This struct wraps a function that takes an environment and a string, and
/// returns `true` if the string is a valid shell variable name in the given
/// environment. An implementation of the function should be provided and stored
/// in the environment's [`any`](Env::any) storage. This allows modules that
/// need to check for variable names to do so without directly depending on the
/// parser crate (`yash-syntax`).
#[derive(Debug)]
pub struct IsName<S>(pub fn(&Env<S>, &str) -> bool);

// Not derived automatically because S may not implement Clone or Copy.
impl<S> Clone for IsName<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for IsName<S> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::option::Option::PosixlyCorrect;
    use crate::option::State::On;

    #[test]
    fn mode_default_permits_all_syntax() {
        assert_eq!(Mode::default(), Mode { portable: false });
    }

    #[test]
    fn mode_from_options_reflects_portable() {
        let mut options = OptionSet::default();
        assert!(!Mode::from(&options).portable);

        options.set(Portable, On);
        assert!(Mode::from(&options).portable);
    }

    #[test]
    fn mode_from_options_ignores_unrelated_options() {
        let mut options = OptionSet::default();
        options.set(PosixlyCorrect, On);
        assert!(!Mode::from(&options).portable);
    }

    #[test]
    fn config_with_input_defaults_to_permissive_mode() {
        let config = Config::with_input(Box::new(crate::input::Memory::new("")));
        assert_eq!(config.mode, Mode::default());
    }
}
