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
//! read [-d delimiter] [-r] variable…
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
//! ## Non-default delimiters
//!
//! By default, the read built-in reads a line up to a newline character. The
//! `-d` option changes the delimiter to the character specified by the
//! `delimiter` value. If the `delimiter` value is empty, the read built-in reads
//! a line up to the first nul byte.
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
//! The **`-d`** (**`--delimiter`**) option takes an argument and changes the
//! delimiter to the character specified by the argument. If the `delimiter`
//! value is empty, the read built-in reads a line up to the first nul byte.
//! Multibyte characters are not supported.
//!
//! The **`-r`** (**`--raw-mode`**) option disables the interpretation of
//! backslashes.
//!
//! # Operands
//!
//! One or more operands are required.
//! Each operand is the name of a variable to be assigned.
//!
//! # Errors
//!
//! This built-in fails if:
//!
//! - The standard input is not readable.
//! - The delimiter is not a single-byte character.
//! - The delimiter is not a nul byte and the input contains a nul byte.
//! - A variable to be assigned is read-only.
//!
//! # Exit status
//!
//! The exit status is zero if a line was read successfully and non-zero
//! otherwise. If the built-in reaches the end of the input before finding a
//! delimiter, the exit status is one, but the variables are still assigned with
//! the line read so far. On other errors, the exit status is two or higher.
//!
//! # Portability
//!
//! The read built-in is defined in the POSIX standard with the `-d` and `-r`
//! options.
//!
//! In this implementation, the value of the `PS2` variable is subject to
//! parameter expansion, command substitution, and arithmetic expansion. Other
//! implementations may not perform these expansions.
//!
//! # Implementation notes
//!
//! The built-in reads the input byte by byte. This is inefficient, but it is
//! necessary not to read past the delimiter.
//! (TODO: Use a buffered reader if the input is seekable)

use crate::common::report;
use crate::common::report_simple;
use crate::common::to_single_message;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;

pub mod assigning;
pub mod input;
pub mod prompt;
pub mod syntax;

/// Exit status when the built-in succeeds
pub const EXIT_STATUS_SUCCESS: ExitStatus = ExitStatus(0);

/// Exit status when the built-in reaches the end of the input before finding a newline
pub const EXIT_STATUS_EOF: ExitStatus = ExitStatus(1);

/// Exit status when the built-in fails to assign a value to a variable
pub const EXIT_STATUS_ASSIGN_ERROR: ExitStatus = ExitStatus(2);

/// Exit status when the built-in fails to read from the input
pub const EXIT_STATUS_READ_ERROR: ExitStatus = ExitStatus(3);

/// Exit status on a command line syntax error
pub const EXIT_STATUS_SYNTAX_ERROR: ExitStatus = ExitStatus(4);

/// Abstract command line arguments of the `read` built-in
///
/// An instance of this struct is created by parsing command line arguments
/// using the [`syntax`] module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// Delimiter specified by the `-d` option
    ///
    /// When the option is not specified, this field is `b'\n'`.
    pub delimiter: u8,

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
        Err(error) => return report(env, &error, EXIT_STATUS_SYNTAX_ERROR).await,
    };

    let (input, newline_found) = match input::read(env, command.is_raw).await {
        Ok(input) => input,
        Err(error) => return report(env, &error, EXIT_STATUS_READ_ERROR).await,
    };

    if input.iter().any(|c| c.value == '\0') {
        return report_simple(env, "input contains a nul byte", EXIT_STATUS_READ_ERROR).await;
    }

    let errors = assigning::assign(env, &input, command.variables, command.last_variable);
    let message = to_single_message(&errors);
    match message {
        None if newline_found => EXIT_STATUS_SUCCESS.into(),
        None => EXIT_STATUS_EOF.into(),
        Some(message) => report(env, message, EXIT_STATUS_ASSIGN_ERROR).await,
    }
}
