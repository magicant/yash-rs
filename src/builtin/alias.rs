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

//! Alias built-in.

use super::*;
use crate::alias::*;
use std::rc::Rc;

/// Implementation of the alias built-in.
pub fn alias_built_in(env: &mut dyn Env, args: Vec<Field>) -> Result<ExitStatus> {
    // TODO support options
    // TODO print alias definitions if there are no operands

    let mut args = args.into_iter();
    args.next(); // ignore the first argument, which is the command name

    if args.as_ref().is_empty() {
        for alias in env.aliases().as_ref() {
            // TODO should print via IoEnv rather than directly to stdout
            println!("{}={}", &alias.0.name, &alias.0.replacement);
        }
        return Ok(0);
    }

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let name = value[..eq_index].to_owned();
            // TODO reject invalid name
            let replacement = value[eq_index + 1..].to_owned();
            Rc::make_mut(env.aliases_mut()).insert(HashEntry::new(
                name,
                replacement,
                false,
                origin,
            ));
        } else {
            // TODO print alias definition
        }
    }

    Ok(0)
}

// TODO test alias_built_in
