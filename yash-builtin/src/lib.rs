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
//! # Optional dependencies
//!
//! The `yash-builtin` crate has an optional dependency on the `yash-semantics`
//! crate, which is enabled by default. If you disable the `yash-semantics`
//! feature, the following built-ins will be unavailable:
//!
//! - `command`
//! - `eval`
//! - `exec`
//! - `read`
//! - `source`
//! - `type`
//! - `wait`
//!
//! The `yash-builtin` crate also has an optional dependency on the
//! `yash-prompt` crate, which is enabled by default. If you disable the
//! `yash-prompt` feature, the `read` built-in will not print the prompt.
//! Note that the `yash-prompt` feature requires the `yash-semantics` feature.

pub mod alias;
pub mod bg;
pub mod r#break;
pub mod cd;
pub mod colon;
#[cfg(feature = "yash-semantics")]
pub mod command;
pub mod common;
pub mod r#continue;
#[cfg(feature = "yash-semantics")]
pub mod eval;
#[cfg(feature = "yash-semantics")]
pub mod exec;
pub mod exit;
pub mod export;
pub mod r#false;
pub mod fg;
pub mod getopts;
pub mod jobs;
pub mod kill;
pub mod pwd;
#[cfg(feature = "yash-semantics")]
pub mod read;
pub mod readonly;
pub mod r#return;
pub mod set;
pub mod shift;
#[cfg(feature = "yash-semantics")]
pub mod source;
pub mod times;
pub mod trap;
pub mod r#true;
#[cfg(feature = "yash-semantics")]
pub mod r#type;
pub mod typeset;
pub mod ulimit;
pub mod umask;
pub mod unalias;
pub mod unset;
#[cfg(feature = "yash-semantics")]
pub mod wait;

#[doc(no_inline)]
pub use yash_env::builtin::*;
#[cfg(doc)]
use yash_env::stack::{Frame, Stack};
#[cfg(doc)]
use yash_env::Env;

use std::future::ready;
use Type::{Elective, Mandatory, Special};

/// Array of all the implemented built-in utilities.
///
/// The array items are ordered alphabetically.
pub const BUILTINS: &[(&str, Builtin)] = &[
    #[cfg(feature = "yash-semantics")]
    (
        ".",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(source::main(env, args)),
        },
    ),
    (
        ":",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(ready(colon::main(env, args))),
        },
    ),
    (
        "alias",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(alias::main(env, args)),
        },
    ),
    (
        "bg",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(bg::main(env, args)),
        },
    ),
    (
        "break",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(r#break::main(env, args)),
        },
    ),
    (
        "cd",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(cd::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "command",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(command::main(env, args)),
        },
    ),
    (
        "continue",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(r#continue::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "eval",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(eval::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "exec",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(exec::main(env, args)),
        },
    ),
    (
        "exit",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(exit::main(env, args)),
        },
    ),
    (
        "export",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(export::main(env, args)),
        },
    ),
    (
        "false",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(r#false::main(env, args)),
        },
    ),
    (
        "fg",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(fg::main(env, args)),
        },
    ),
    (
        "getopts",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(getopts::main(env, args)),
        },
    ),
    (
        "jobs",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(jobs::main(env, args)),
        },
    ),
    (
        "kill",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(kill::main(env, args)),
        },
    ),
    (
        "pwd",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(pwd::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "read",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(read::main(env, args)),
        },
    ),
    (
        "readonly",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(readonly::main(env, args)),
        },
    ),
    (
        "return",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(r#return::main(env, args)),
        },
    ),
    (
        "set",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(set::main(env, args)),
        },
    ),
    (
        "shift",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(shift::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "source",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(source::main(env, args)),
        },
    ),
    (
        "times",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(times::main(env, args)),
        },
    ),
    (
        "trap",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(trap::main(env, args)),
        },
    ),
    (
        "true",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(r#true::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "type",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(r#type::main(env, args)),
        },
    ),
    (
        "typeset",
        Builtin {
            r#type: Elective,
            execute: |env, args| Box::pin(typeset::main(env, args)),
        },
    ),
    (
        "ulimit",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(ulimit::main(env, args)),
        },
    ),
    (
        "umask",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(umask::main(env, args)),
        },
    ),
    (
        "unalias",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(unalias::main(env, args)),
        },
    ),
    (
        "unset",
        Builtin {
            r#type: Special,
            execute: |env, args| Box::pin(unset::main(env, args)),
        },
    ),
    #[cfg(feature = "yash-semantics")]
    (
        "wait",
        Builtin {
            r#type: Mandatory,
            execute: |env, args| Box::pin(wait::main(env, args)),
        },
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_sorted() {
        BUILTINS
            .windows(2)
            .for_each(|pair| assert!(pair[0].0 < pair[1].0, "disordered pair: {pair:?}"))
    }
}
