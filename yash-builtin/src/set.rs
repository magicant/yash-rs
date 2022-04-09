// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Set built-in
//!
//! The set built-in modifies [shell options](yash_env::option) and [positional
//! parameters](yash_env::variable). It also can print a list of current options
//! or variables.
//!
//! # Syntax and semantics
//!
//! ## Printing variables
//!
//! ```sh
//! set
//! ```
//!
//! When executed without any arguments, the built-in prints a list of
//! [variables](yash_env::variable) visible in the current execution
//! environment. The list is formatted as a sequence of simple commands
//! performing an assignment that would restore the present variables if
//! executed (unless the assignment fails because of a read-only variable).
//! The list is ordered alphabetically.
//!
//! ## Printing options
//!
//! ```sh
//! set -o
//! ```
//!
//! If you specify the `-o` option as a unique argument to the set built-in, it
//! prints the current option settings in a human-readable format.
//!
//! ```sh
//! set +o
//! ```
//!
//! If you use the `+o` option instead, the printing lists shell commands that
//! would restore the current option settings if executed.
//!
//! ## Modifying shell options
//!
//! Other options modify [shell option](yash_env::option::Option) settings. They
//! can be specified in the short form like `-e` or the long form like `-o
//! errexit` and `--errexit`.
//!
//! You can also specify options starting with `+` in place of `-`, as in `+e`,
//! `+o errexit`, and `++errexit`. The `-` options turn on the corresponding
//! shell options while the `+` options turn off.
//!
//! See [`parse_short`] for a list of available short options and [`parse_long`]
//! to learn how long options are parsed.
//! Long options are [canonicalize]d before being passed to `parse_long`.
//!
//! You cannot modify the following options with the set built-in:
//!
//! - `CmdLine` (`-c`, `-o cmdline`)
//! - `Interactive` (`-i`, `-o interactive`)
//! - `Stdin` (`-s`, `-o stdin`)
//!
//! ## Modifying positional parameters
//!
//! If you specify one or more operands, they will be new positional parameters
//! in the current [context](yash_env::variable), replacing any existing
//! positional parameters.
//!
//! ## Option-operand separator
//!
//! As with other utilities conforming to POSIX XBD Utility Syntax Guidelines,
//! the set built-in accepts `--` as a separator between options and operands.
//! Additionally, you can separate them with `-` instead of `--`.
//!
//! If you place a separator without any operands, the built-in will clear all
//! positional parameters.
//!
//! # Exit status
//!
//! - 0: successful
//! - 1: error printing output
//! - 2: invalid options
//!
//! # Portability
//!
//! POSIX defines only the following option names:
//!
//! - `-a`, `-o allexport`
//! - `-b`, `-o notify`
//! - `-C`, `-o noclobber`
//! - `-e`, `-o errexit`
//! - `-f`, `-o noglob`
//! - `-h`
//! - `-m`, `-o monitor`
//! - `-n`, `-o noexec`
//! - `-u`, `-o nounset`
//! - `-v`, `-o verbose`
//! - `-x`, `-o xtrace`
//!
//! Other options (including non-canonicalized ones) are not portable. Also,
//! using the `no` prefix to negate an arbitrary option is not portable. For
//! example, `+o noexec` is portable, but `-o exec` is not.
//!
//! The output format of `set -o` and `set +o` depends on the shell.
//!
//! The semantics of `-` as an option-operand separator is unspecified in POSIX.
//! You should prefer `--`.
//!
//! Many (but not all) shells specially treat `+`, especially when it appears in
//! place of an option-operand separator. This behavior is not portable either.

use crate::common::BuiltinName;
use std::future::ready;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
#[cfg(doc)]
use yash_env::option::canonicalize;
#[cfg(doc)]
use yash_env::option::parse_long;
#[cfg(doc)]
use yash_env::option::parse_short;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::Array;
use yash_env::Env;

pub mod arg;

/// Implementation of the set built-in.
pub fn builtin_main_sync(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO Parse options
    // TODO Print existing variables when no arguments are given

    let location = env.builtin_name().origin.clone();
    let params = env.variables.positional_params_mut();
    params.value = Array(args.into_iter().map(|f| f.value).collect());
    params.last_assigned_location = Some(location);

    (ExitStatus::SUCCESS, Continue(()))
}

/// Asynchronous wrapper of [`builtin_main_sync`].
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::stack::Frame;

    #[test]
    fn setting_some_positional_parameters() {
        let name = Field::dummy("set");
        let location = name.origin.clone();
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(Frame::Builtin { name });
        let args = Field::dummies(["a", "b", "z"]);

        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let v = env.variables.positional_params();
        assert_eq!(
            v.value,
            Array(vec!["a".to_string(), "b".to_string(), "z".to_string()])
        );
        assert_eq!(v.read_only_location, None);
        assert_eq!(v.last_assigned_location.as_ref().unwrap(), &location);
    }
}
