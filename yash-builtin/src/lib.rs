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
//! TODO Elaborate

pub mod alias;
pub mod common;
pub mod readonly;
pub mod r#return;
pub mod set;
pub mod trap;

#[doc(no_inline)]
pub use yash_env::builtin::*;

use Type::{Intrinsic, Special};

/// Array of all the implemented built-in utilities.
pub const BUILTINS: &[(&str, Builtin)] = &[
    (
        "alias",
        Builtin {
            r#type: Intrinsic,
            execute: alias::builtin_main,
        },
    ),
    (
        "readonly",
        Builtin {
            r#type: Special,
            execute: readonly::builtin_main,
        },
    ),
    (
        "return",
        Builtin {
            r#type: Special,
            execute: r#return::builtin_main,
        },
    ),
    (
        "set",
        Builtin {
            r#type: Special,
            execute: set::builtin_main,
        },
    ),
    (
        "trap",
        Builtin {
            r#type: Special,
            execute: trap::builtin_main,
        },
    ),
];
