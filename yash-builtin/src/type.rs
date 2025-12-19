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
//! This module implements the [`type` built-in], which identifies the type of commands.
//!
//! [`type` built-in]: https://magicant.github.io/yash-rs/builtins/type.html

use crate::command::Command;
use crate::command::syntax::interpret;
use crate::common::report::report_error;
use crate::common::syntax::Mode;
use crate::common::syntax::OptionOccurrence;
use crate::common::syntax::OptionSpec;
use crate::common::syntax::parse_arguments;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::source::Location;
use yash_env::system::System;

const OPTION_SPECS: &[OptionSpec] = &[
    // TODO: Non-standard options
];

fn parse<S>(env: &mut Env<S>, args: Vec<Field>) -> Result<Command, crate::command::syntax::Error> {
    let (mut options, operands) = parse_arguments(OPTION_SPECS, Mode::with_env(env), args)?;

    // `type` is equivalent to `command -V`, so add the `-V` option and delegate
    // to the `command` built-in.
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
pub async fn main<S: System + 'static>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result {
    match parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
