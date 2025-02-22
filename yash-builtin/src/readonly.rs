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
//! TODO: **Only portable features are implemented for now.**
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
//! variables named by the operands in the format that can be
//! [evaluated](crate::eval) as shell code to recreate the variables.
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
//! For array variables, the readonly built-in invocation is preceded by a
//! separate assignment command since the readonly built-in does not support
//! assigning values to array variables.
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
//! by the operands in the format that can be [evaluated](crate::eval) as shell
//! code to recreate the functions.
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
//! # Exit status
//!
//! Zero unless an error occurs.
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

use crate::common::output;
use crate::common::report_error;
use crate::common::report_failure;
use crate::common::to_single_message;
use crate::typeset::Command;
use crate::typeset::FunctionAttr;
use crate::typeset::PrintContext;
use crate::typeset::Scope::Global;
use crate::typeset::VariableAttr;
use crate::typeset::syntax::OptionSpec;
use crate::typeset::syntax::PRINT_OPTION;
use crate::typeset::syntax::interpret;
use crate::typeset::syntax::parse;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::option::State::On;
use yash_env::semantics::Field;

/// List of portable options applicable to the readonly built-in
pub const PORTABLE_OPTIONS: &[OptionSpec<'static>] = &[PRINT_OPTION];

/// Printing context for the readonly built-in
pub const PRINT_CONTEXT: PrintContext<'static> = PrintContext {
    builtin_name: "readonly",
    builtin_is_significant: true,
    options_allowed: &[],
};

// TODO Split into syntax and semantics submodules

/// Entry point for executing the `readonly` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    match parse(PORTABLE_OPTIONS, args) {
        Ok((options, operands)) => match interpret(options, operands) {
            Ok(mut command) => {
                match &mut command {
                    Command::SetVariables(sv) => {
                        sv.attrs.push((VariableAttr::ReadOnly, On));
                        sv.scope = Global;
                    }
                    Command::PrintVariables(pv) => {
                        pv.attrs.push((VariableAttr::ReadOnly, On));
                        pv.scope = Global;
                    }
                    Command::SetFunctions(sf) => sf.attrs.push((FunctionAttr::ReadOnly, On)),
                    Command::PrintFunctions(pf) => pf.attrs.push((FunctionAttr::ReadOnly, On)),
                }
                match command.execute(env, &PRINT_CONTEXT) {
                    Ok(result) => output(env, &result).await,
                    Err(errors) => report_failure(env, to_single_message(&errors).unwrap()).await,
                }
            }
            Err(error) => report_error(env, &error).await,
        },
        Err(error) => report_error(env, &error).await,
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use yash_env::Env;
    use yash_env::semantics::ExitStatus;
    use yash_env::variable::Value;

    #[test]
    fn builtin_defines_read_only_variable() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["foo=bar baz"]);
        let location = args[0].origin.clone();

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        let v = env.variables.get("foo").unwrap();
        assert_eq!(v.value, Some(Value::scalar("bar baz")));
        assert_eq!(v.is_exported, false);
        assert_eq!(v.read_only_location.as_ref().unwrap(), &location);
        assert_eq!(v.last_assigned_location.as_ref().unwrap(), &location);
    }
}
