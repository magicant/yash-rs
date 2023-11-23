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

//! Readonly built-in.
//!
//! The **`readonly`** built-in behaves differently depending on the arguments.
//!
//! # Making variables read-only
//!
//! If the `-p` (`--print`) or `-f` (`--functions`) option is not specified and
//! there are any operands, the built-in makes the specified variables
//! read-only.
//!
//! ## Synopsis
//!
//! ```sh
//! readonly name[=value]…
//! ```
//!
//! ## Options
//!
//! None.
//!
//! ## Operands
//!
//! Operands specify the names and values of the variables to be made read-only.
//! If an operand contains an equal sign (`=`), the operand is split into the
//! name and value at the first equal sign. The value is assigned to the
//! variable named by the name. Otherwise, the variable named by the operand is
//! created without a value unless it is already defined, in which case the
//! existing value is retained.
//!
//! If no operands are given, the built-in prints variables (see below).
//!
//! ## Standard output
//!
//! None.
//!
//! # Printing read-only variables
//!
//! If the `-p` (`--print`) option is specified and the `-f` (`--functions`)
//! option is not specified, the built-in prints the names and values of the
//! variables named by the operands in the format that can be evaluated as shell
//! code to recreate the variables.
//! <!-- TODO: link to the eval built-in -->
//! If there are no operands and the `-f` (`--functions`) option is not
//! specified, the built-in prints all read-only variables in the same format.
//!
//! ## Synopsis
//!
//! ```sh
//! readonly -p [name…]
//! ```
//!
//! ```sh
//! readonly
//! ```
//!
//! # Options
//!
//! The **`-p`** (**`--print`**) option must be specified to print variables
//! unless there are no operands.
//!
//! # Operands
//!
//! Operands specify the names of the variables to be printed. If no operands
//! are given, all read-only variables are printed.
//!
//! ## Standard output
//!
//! A command string that invokes the readonly built-in to recreate the variable
//! is printed for each read-only variable. Note that the command does not
//! include options to restore the attributes of the variable, such as the `-x`
//! option to make variables exported.
//!
//! Also note that evaluating the printed commands in the current context will
//! fail (unless the variable is declared without a value) because the variable
//! is already defined and read-only.
//!
//! # Making functions read-only
//!
//! If the `-f` (`--functions`) option is specified, the built-in makes the
//! specified functions read-only.
//!
//! ## Synopsis
//!
//! ```sh
//! readonly -f name…
//! ```
//!
//! ## Options
//!
//! The **`-f`** (**`--functions`**) option must be specified to make functions
//! read-only.
//!
//! ## Operands
//!
//! Operands specify the names of the functions to be made read-only.
//!
//! ## Standard output
//!
//! None.
//!
//! # Printing read-only functions
//!
//! If the `-f` (`--functions`) and `-p` (`--print`) options are specified, the
//! built-in prints the attributes and definitions of the shell functions named
//! by the operands in the format that can be evaluated as shell code to
//! recreate the functions.
//! <!-- TODO: link to the eval built-in -->
//! If there are no operands and the `-f` (`--functions`) option is specified,
//! the built-in prints all read-only functions in the same format.
//!
//! ## Synopsis
//!
//! ```sh
//! readonly -fp [name…]
//! ```
//!
//! ```sh
//! readonly -f
//! ```
//!
//! ## Options
//!
//! The **`-f`** (**`--functions`**) and **`-p`** (**`--print`**) options must be
//! specified to print functions. The `-p` option may be omitted if there are no
//! operands.
//!
//! ## Operands
//!
//! Operands specify the names of the functions to be printed. If no operands
//! are given, all read-only functions are printed.
//!
//! ## Standard output
//!
//! A command string of a function definition command is printed for each
//! function, followed by a simple command invoking the readonly built-in to
//! make the function read-only.
//!
//! Note that executing the printed commands in the current context will fail
//! because the function is already defined and read-only.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Errors
//!
//! When making a variable read-only with a value, it is an error if the
//! variable is already read-only.
//!
//! It is an error to specify a non-existing function for making it read-only.
//!
//! When printing variables or functions, it is an error if an operand names a
//! non-existing variable or function.
//!
//! # Portability
//!
//! This built-in is part of the POSIX standard. Printing variables is portable
//! only when the `-p` option is used without operands. Operations on functions
//! with the `-f` option are non-portable extensions.
//!
//! # Implementation notes
//!
//! The implementation of this built-in depends on that of the
//! [`typeset`](crate::typeset) built-in. The readonly built-in basically works
//! like the typeset built-in with the `-r` (`--readonly`) option except that:
//! - The `-g` (`--global`) option is also implied when operating on variables.
//! - Printed commands name the readonly built-in instead of the typeset built-in.
//! - Printed commands do not include options that modify variable attributes.

use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::Scope;
use yash_env::Env;

// TODO Split into syntax and semantics submodules

/// Entry point for executing the `readonly` built-in
pub fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO support options
    // TODO print read-only variables if there are no operands

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let var_value = value[eq_index + 1..].to_owned();

            let mut name = value;
            name.truncate(eq_index);
            // TODO reject invalid name

            let mut var = env.get_or_create_variable(name.clone(), Scope::Global);
            match var.assign(var_value, origin.clone()) {
                Ok(_) => var.make_read_only(origin),
                Err(_) => {
                    // TODO Better error message
                    // TODO Use Env rather than printing directly to stderr
                    eprintln!("cannot assign to read-only variable {name}");
                    return ExitStatus::FAILURE.into();
                }
            }
        } else {
            // TODO Make an existing variable read-only or create a new value-less variable
        }
    }

    ExitStatus::SUCCESS.into()
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::variable::Value;
    use yash_env::Env;

    #[test]
    fn builtin_defines_read_only_variable() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["foo=bar baz"]);
        let location = args[0].origin.clone();

        let result = main(&mut env, args);
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        let v = env.variables.get("foo").unwrap();
        assert_eq!(v.value, Some(Value::scalar("bar baz")));
        assert_eq!(v.is_exported, false);
        assert_eq!(v.read_only_location.as_ref().unwrap(), &location);
        assert_eq!(v.last_assigned_location.as_ref().unwrap(), &location);
    }
}
