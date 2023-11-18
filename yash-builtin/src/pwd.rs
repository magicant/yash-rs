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

//! Pwd built-in.
//!
//! The **`pwd`** built-in prints the working directory path.
//!
//! # Synopsis
//!
//! ```sh
//! pwd [-L|-P]
//! ```
//!
//! # Description
//!
//! The built-in prints the pathname of the current working directory followed
//! by a newline to the standard output.
//!
//! # Options
//!
//! With the **`-L`** (**`--logical`**) option, the printed path is the value of
//! `$PWD` if it is correct ([`Env::has_correct_pwd`]). The path may contain
//! symbolic link components.
//!
//! With the **`-P`** (**`--physical`**) option (or if `$PWD` is not correct),
//! the built-in recomputes and prints the canonical path to the working
//! directory.
//!
//! These two options are mutually exclusive. The last specified one applies if
//! given both. The default is `-L`.
//!
//! # Operands
//!
//! None.
//!
//! # Exit Status
//!
//! Zero if the path was successfully printed; non-zero otherwise.
//!
//! # Errors
//!
//! This built-in may fail for various reasons. For example:
//! - The working directory has been removed from the file system.
//! - You do not have permission to access the ancestor directories of the working directory.
//! - The standard output is not writable.
//!
//! # Portability
//!
//! The `-L` and `-P` options are defined in POSIX.
//!
//! POSIX allows the built-in to apply the `-P` option if the `-L` option is
//! specified and `$PWD` is longer than PATH_MAX.
//!
//! The shell sets `$PWD` on the startup and modifies it in the [cd](crate::cd)
//! built-in. If `$PWD` is modified or unset otherwise, the behavior of the cd
//! and pwd built-ins is unspecified.
//!
//! # Implementation notes
//!
//! The result for the `-P` option is obtained with [`System::getcwd`].

use crate::common::arrange_message_and_divert;
use crate::common::output;
use crate::common::report_error;
use std::borrow::Cow;
use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
#[cfg(doc)]
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

/// Choice of the behavior of the built-in
#[derive(Debug, Clone, Copy, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Mode {
    /// The built-in prints the value of `$PWD` if it is
    /// [correct](Env::has_correct_pwd).
    ///
    /// If `$PWD` is not a correct path, the built-in falls back to
    /// [`Physical`](Self::Physical).
    #[default]
    Logical,

    /// The built-in computes the canonical path to the working directory.
    Physical,
}

pub mod semantics;
pub mod syntax;

async fn report_semantics_error(env: &mut Env, error: &semantics::Error) -> Result {
    let location = env.stack.current_builtin().map_or_else(
        || Cow::Owned(Location::dummy("")),
        |field| Cow::Borrowed(&field.name.origin),
    );
    let annotation = Annotation::new(AnnotationType::Error, error.to_string().into(), &location);
    let message = Message {
        r#type: AnnotationType::Error,
        title: "cannot compute the working directory path".into(),
        annotations: vec![annotation],
    };
    let (message, divert) = arrange_message_and_divert(env, message);
    env.system.print_error(&message).await;
    Result::with_exit_status_and_divert(ExitStatus::FAILURE, divert)
}

/// Entry point for executing the `pwd` built-in
///
/// This function uses the [`syntax`] and [`semantics`] modules to execute the built-in.
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    match syntax::parse(env, args) {
        Ok(mode) => match semantics::compute(env, mode) {
            Ok(result) => output(env, &result).await,
            Err(e) => report_semantics_error(env, &e).await,
        },
        Err(e) => report_error(env, &e).await,
    }
}
