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

//! Read built-in
//!
//! The **`read`** built-in reads a line into variables.
//!
//! # Synopsis
//!
//! ```sh
//! read [-r] variable…
//! ```
//!
//! # Description
//!
//! The read built-in reads a line from the standard input and assigns it to the
//! variables named by the operands. Field splitting is performed on the line
//! read to produce as many fields as there are variables. If there are fewer
//! fields than variables, the remaining variables are set to empty strings. If
//! there are more fields than variables, the last variable receives all
//! remaining fields, including the field separators, but not trailing
//! whitespace separators.
//!
//! ## Escaping
//!
//! By default, backslashes in the input are treated as quoting characters that
//! prevent the following character from being interpreted as a field separator.
//! Backslash-newline pairs are treated as line continuations.
//!
//! The `-r` option disables this behavior.
//!
//! ## Prompting
//!
//! By default, the read built-in does not display a prompt before reading a
//! line. (TODO: Options to display a prompt)
//!
//! When reading lines after the first line, the read built-in displays the
//! value of the `PS2` variable as a prompt if the shell is interactive and the
//! input is from a terminal.
//!
//! Prompting requires the optional `yash-prompt` feature.
//!
//! # Options
//!
//! The **`-r`** option disables the interpretation of backslashes.
//!
//! # Operands
//!
//! One or more operands are required.
//! Each operand is the name of a variable to be assigned.
//!
//! # Errors
//!
//! It is an error if the standard input is not readable.
//!
//! It is an error if any variable to be assigned is read-only.
//!
//! # Exit status
//!
//! The exit status is zero if a line was read successfully and non-zero
//! otherwise. If the built-in reaches the end of the input before finding a
//! newline, it returns non-zero, but the variables are still assigned with the
//! line read so far.
//!
//! # Portability
//!
//! The read built-in is defined in the POSIX standard. The `-r` option is the
//! only option defined in the POSIX standard.
//!
//! In this implementation, the value of the `PS2` variable is subject to
//! parameter expansion, command substitution, and arithmetic expansion. Other
//! implementations may not perform these expansions.
//!
//! # Implementation notes
//!
//! Reading from an unseekable input may be slow because the built-in reads the
//! input byte by byte to make sure it does not read past the end of the line.

use crate::common::report_error;
use crate::common::report_failure;
use crate::common::to_single_message;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;

pub mod assigning;
pub mod input;
pub mod prompt;
pub mod syntax;

/// Abstract command line arguments of the `read` built-in
///
/// An instance of this struct is created by parsing command line arguments
/// using the [`syntax`] module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// Whether the `-r` option is specified
    ///
    /// If this field is `true`, backslashes are not interpreted.
    pub is_raw: bool,

    /// Names of variables to be assigned, except the last one
    pub variables: Vec<Field>,

    /// Name of the last variable to be assigned
    ///
    /// The last variable receives all remaining fields, including the
    /// intermediate (but not trailing) field separators.
    pub last_variable: Field,
}

/// Entry point of the `read` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    let command = match syntax::parse(env, args) {
        Ok(command) => command,
        Err(error) => return report_error(env, &error).await,
    };

    let (input, newline_found) = match input::read(env, command.is_raw).await {
        Ok(input) => input,
        Err(error) => return report_failure(env, &error).await,
    };

    let errors = assigning::assign(env, &input, command.variables, command.last_variable);
    let message = to_single_message(&errors);
    match message {
        None if newline_found => ExitStatus::SUCCESS.into(),
        None => ExitStatus::FAILURE.into(),
        Some(message) => report_failure(env, message).await,
    }
}
