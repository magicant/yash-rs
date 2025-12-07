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

//! Implementation of the shell built-in utilities.
//!
//! Each built-in utility is implemented in the submodule named after the
//! utility. The submodule contains the `main` function that implements the
//! built-in utility. The submodule many also export other items that are used
//! by the `main` function. The module documentation for each submodule
//! describes the specification of the built-in utility.
//!
//! The [`common`] module provides common functions that are used for
//! implementing built-in utilities.
//!
//! # Stack
//!
//! Many built-ins in this crate use [`Stack::current_builtin`] to obtain the
//! command word that invoked the built-in. It is used to report the command
//! location in error messages, switch the behavior of the built-in depending on
//! the command, etc. For the built-ins to work correctly, the
//! [stack](Env::stack) should contain a [built-in frame](Frame::Builtin) so
//! that `Stack::current_builtin` provides the correct command word.
//!
//! # Dependencies to be injected
//!
//! Some built-ins in this crate require certain dependencies to be injected
//! into the environment's [`any`](yash_env::Env::any) storage. If these
//! dependencies are not injected, the built-in may **panic** at runtime.
//!
//! - The `command` built-in requires a
//!   [`RunFunction`](yash_env::semantics::command::RunFunction) instance to
//!   invoke shell functions, and an [`IsKeyword`](yash_env::parser::IsKeyword)
//!   instance to check for reserved words.
//! - The `eval` and `source` built-ins require a
//!   [`RunReadEvalLoop`](yash_env::semantics::RunReadEvalLoop) instance to run
//!   the read-eval loop for executing commands.
//! - The `read` built-in requires a [`GetPrompt`](yash_env::prompt::GetPrompt)
//!   instance to generate prompts when reading input.
//! - The `wait` built-in requires a
//!   [`RunSignalTrapIfCaught`](yash_env::trap::RunSignalTrapIfCaught) instance
//!   to handle trapped signals while waiting for jobs.

pub mod alias;
pub mod bg;
pub mod r#break;
pub mod cd;
pub mod colon;
pub mod command;
pub mod common;
pub mod r#continue;
pub mod eval;
pub mod exec;
pub mod exit;
pub mod export;
pub mod r#false;
pub mod fg;
pub mod getopts;
pub mod jobs;
pub mod kill;
pub mod pwd;
pub mod read;
pub mod readonly;
pub mod r#return;
pub mod set;
pub mod shift;
pub mod source;
pub mod times;
pub mod trap;
pub mod r#true;
pub mod r#type;
pub mod typeset;
pub mod ulimit;
pub mod umask;
pub mod unalias;
pub mod unset;
pub mod wait;

#[cfg(doc)]
use yash_env::Env;
#[doc(no_inline)]
pub use yash_env::builtin::*;
#[cfg(doc)]
use yash_env::stack::{Frame, Stack};
use yash_env::system::System;

use Type::{Elective, Mandatory, Special, Substitutive};
use std::future::ready;

/// Returns an iterator over all the implemented built-in utilities.
///
/// Each item yielded by the iterator is a tuple of the built-in name and
/// corresponding [`Builtin`] instance. The items are sorted by the built-in
/// names in ascending order.
pub fn iter<S>() -> impl Iterator<Item = (&'static str, Builtin<S>)>
where
    S: System + 'static,
{
    [
        (
            ".",
            Builtin::new(Special, |env, args| Box::pin(source::main(env, args))),
        ),
        (
            ":",
            Builtin::new(Special, |env, args| Box::pin(ready(colon::main(env, args)))),
        ),
        (
            "alias",
            Builtin::new(Mandatory, |env, args| Box::pin(alias::main(env, args))),
        ),
        (
            "bg",
            Builtin::new(Mandatory, |env, args| Box::pin(bg::main(env, args))),
        ),
        (
            "break",
            Builtin::new(Special, |env, args| Box::pin(r#break::main(env, args))),
        ),
        (
            "cd",
            Builtin::new(Mandatory, |env, args| Box::pin(cd::main(env, args))),
        ),
        ("command", {
            let mut builtin =
                Builtin::new(Mandatory, |env, args| Box::pin(command::main(env, args)));
            builtin.is_declaration_utility = None;
            builtin
        }),
        (
            "continue",
            Builtin::new(Special, |env, args| Box::pin(r#continue::main(env, args))),
        ),
        (
            "eval",
            Builtin::new(Special, |env, args| Box::pin(eval::main(env, args))),
        ),
        (
            "exec",
            Builtin::new(Special, |env, args| Box::pin(exec::main(env, args))),
        ),
        (
            "exit",
            Builtin::new(Special, |env, args| Box::pin(exit::main(env, args))),
        ),
        ("export", {
            let mut builtin = Builtin::new(Special, |env, args| Box::pin(export::main(env, args)));
            builtin.is_declaration_utility = Some(true);
            builtin
        }),
        (
            "false",
            Builtin::new(Substitutive, |env, args| Box::pin(r#false::main(env, args))),
        ),
        (
            "fg",
            Builtin::new(Mandatory, |env, args| Box::pin(fg::main(env, args))),
        ),
        (
            "getopts",
            Builtin::new(Mandatory, |env, args| Box::pin(getopts::main(env, args))),
        ),
        (
            "jobs",
            Builtin::new(Mandatory, |env, args| Box::pin(jobs::main(env, args))),
        ),
        (
            "kill",
            Builtin::new(Mandatory, |env, args| Box::pin(kill::main(env, args))),
        ),
        (
            "pwd",
            Builtin::new(Substitutive, |env, args| Box::pin(pwd::main(env, args))),
        ),
        (
            "read",
            Builtin::new(Mandatory, |env, args| Box::pin(read::main(env, args))),
        ),
        ("readonly", {
            let mut builtin =
                Builtin::new(Special, |env, args| Box::pin(readonly::main(env, args)));
            builtin.is_declaration_utility = Some(true);
            builtin
        }),
        (
            "return",
            Builtin::new(Special, |env, args| Box::pin(r#return::main(env, args))),
        ),
        (
            "set",
            Builtin::new(Special, |env, args| Box::pin(set::main(env, args))),
        ),
        (
            "shift",
            Builtin::new(Special, |env, args| Box::pin(shift::main(env, args))),
        ),
        (
            "source",
            Builtin::new(Special, |env, args| Box::pin(source::main(env, args))),
        ),
        (
            "times",
            Builtin::new(Special, |env, args| Box::pin(times::main(env, args))),
        ),
        (
            "trap",
            Builtin::new(Special, |env, args| Box::pin(trap::main(env, args))),
        ),
        (
            "true",
            Builtin::new(Substitutive, |env, args| Box::pin(r#true::main(env, args))),
        ),
        (
            "type",
            Builtin::new(Mandatory, |env, args| Box::pin(r#type::main(env, args))),
        ),
        ("typeset", {
            let mut builtin =
                Builtin::new(Elective, |env, args| Box::pin(typeset::main(env, args)));
            builtin.is_declaration_utility = Some(true);
            builtin
        }),
        (
            "ulimit",
            Builtin::new(Mandatory, |env, args| Box::pin(ulimit::main(env, args))),
        ),
        (
            "umask",
            Builtin::new(Mandatory, |env, args| Box::pin(umask::main(env, args))),
        ),
        (
            "unalias",
            Builtin::new(Mandatory, |env, args| Box::pin(unalias::main(env, args))),
        ),
        (
            "unset",
            Builtin::new(Special, |env, args| Box::pin(unset::main(env, args))),
        ),
        (
            "wait",
            Builtin::new(Mandatory, |env, args| Box::pin(wait::main(env, args))),
        ),
    ]
    .iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::system::r#virtual::VirtualSystem;

    #[test]
    fn iter_is_sorted() {
        assert!(iter::<VirtualSystem>().is_sorted_by_key(|pair| pair.0));
    }
}
