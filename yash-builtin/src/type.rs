// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Type built-in
//!
//! The **`type`** built-in identifies the type of commands.
//!
//! # Synopsis
//!
//! ```sh
//! type [name…]
//! ```
//!
//! # Description
//!
//! The `type` built-in prints the description of the specified command names.
//!
//! # Options
//!
//! (TODO: Non-standard options are not supported yet.)
//!
//! # Operands
//!
//! The ***name*** operands specify the command names to identify.
//!
//! # Standard output
//!
//! The command descriptions are printed to the standard output.
//!
//! # Errors
//!
//! It is an error if the *name* is not found.
//!
//! # Exit status
//!
//! The exit status is zero if all the *name*s are found, and non-zero
//! otherwise.
//!
//! # Portability
//!
//! POSIX requires that the *name* operand be specified, but many
//! implementations allow it to be omitted, in which case the built-in does
//! nothing.
//!
//! The format of the output is unspecified by POSIX. In this implementation,
//! the `type` built-in is equivalent to the [`command`] built-in with the `-V`
//! option.
//!
//! [`command`]: crate::command

use crate::command::syntax::interpret;
use crate::command::Command;
use crate::common::report_error;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionOccurrence;
use crate::common::syntax::OptionSpec;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::source::Location;

const OPTION_SPECS: &[OptionSpec] = &[
    // TODO: Non-standard options
];

fn parse(env: &mut Env, args: Vec<Field>) -> Result<Command, crate::command::syntax::Error> {
    let (mut options, operands) = parse_arguments(OPTION_SPECS, Mode::with_env(env), args)?;
    let spec = OptionSpec::new().short('V').long("verbose-identify");
    let location = env.stack.current_builtin().map_or_else(
        || Location::dummy(""),
        |builtin| builtin.name.origin.clone(),
    );
    options.push(OptionOccurrence {
        spec: &spec,
        location,
        argument: None,
    });
    interpret(options, operands)
}

/// Entry point of the `type` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
