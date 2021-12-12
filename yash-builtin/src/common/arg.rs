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

//! Command-line argument parser
//!
//! This module provides functionalities for parsing command-line arguments into
//! options and operands.
//!
//! This module's parser can parse command lines that adhere to POSIX Utility
//! Syntax Guidelines and support non-standard syntax extensions such as long
//! options and options after operands.
//!
//! # Example
//!
//! TODO

#[doc(no_inline)]
pub use yash_env::semantics::Field;

/// TODO
pub struct OptionSpec {}

/// TODO
pub struct Mode {}

/// TODO
pub struct ParsedOption {}

/// TODO
pub struct Error {}

/// Parses command-line arguments into options and operands.
pub fn parse_arguments(
    _option_specs: &[OptionSpec],
    _mode: Mode,
    _arguments: Vec<Field>,
) -> Result<(Vec<ParsedOption>, Vec<Field>), Error> {
    todo!()
}
