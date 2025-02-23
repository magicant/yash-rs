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

//! Export built-in
//!
//! The **`export`** built-in exports shell variables to the environment.
//!
//! # Synopsis
//!
//! ```sh
//! export [-p] [name[=value]…]
//! ```
//!
//! # Description
//!
//! The export built-in (without the `-p` option) exports variables of the
//! specified names to the environment, with optional values. If no names are
//! given, or if the `-p` option is given, the names and values of all exported
//! variables are displayed. If the `-p` option is given with operands, only the
//! specified variables are displayed.
//!
//! # Options
//!
//! The **`-p`** (**`--print`**) option causes the shell to display the names and
//! values of all exported variables in a format that can be reused as input to
//! restore the state of these variables. When used with operands, the option
//! limits the output to the specified variables.
//!
//! (TODO: Other non-portable options)
//!
//! # Operands
//!
//! The operands are the names of shell variables to be exported or printed.
//! When exporting, each name may optionally be followed by `=` and a *value* to
//! assign to the variable.
//!
//! # Standard output
//!
//! When exporting variables, the export built-in does not produce any output.
//!
//! When printing variables, the built-in prints simple commands that invoke the
//! export built-in to reexport the variables with the same values.
//! Note that the commands do not include options to restore the attributes of
//! the variables, such as the `-r` option to make variables read-only.
//!
//! For array variables, the export built-in invocation is preceded by a
//! separate assignment command since the export built-in does not support
//! assigning values to array variables.
//!
//! # Errors
//!
//! When exporting a variable with a value, it is an error if the variable is
//! read-only.
//!
//! When printing variables, it is an error if an operand names a non-existing
//! variable.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Portability
//!
//! This built-in is part of the POSIX standard. Printing variables is portable
//! only when the `-p` option is used without operands.
//!
//! # Implementation notes
//!
//! The implementation of this built-in depends on that of the
//! [`typeset`](crate::typeset) built-in. The export built-in basically works
//! like the typeset built-in with the `-gx` (`--global --export`) options,
//! except that:
//! - Printed commands name the export built-in instead of the typeset built-in.
//! - Printed commands do not include options that modify variable attributes.

use crate::common::output;
use crate::common::report_error;
use crate::common::report_failure;
use crate::common::to_single_message;
use crate::typeset::Command;
use crate::typeset::PrintContext;
use crate::typeset::Scope::Global;
use crate::typeset::VariableAttr::Export;
use crate::typeset::syntax::OptionSpec;
use crate::typeset::syntax::PRINT_OPTION;
use crate::typeset::syntax::interpret;
use crate::typeset::syntax::parse;
use yash_env::Env;
use yash_env::option::State::On;
use yash_env::semantics::Field;

/// List of portable options applicable to the export built-in
pub const PORTABLE_OPTIONS: &[OptionSpec<'static>] = &[PRINT_OPTION];

/// Printing context for the export built-in
pub const PRINT_CONTEXT: PrintContext<'static> = PrintContext {
    builtin_name: "export",
    builtin_is_significant: true,
    options_allowed: &[],
};

/// Entry point of the export built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> yash_env::builtin::Result {
    match parse(PORTABLE_OPTIONS, args) {
        Ok((options, operands)) => match interpret(options, operands) {
            Ok(mut command) => {
                match &mut command {
                    Command::SetVariables(sv) => {
                        sv.attrs.push((Export, On));
                        sv.scope = Global;
                    }
                    Command::PrintVariables(pv) => {
                        pv.attrs.push((Export, On));
                        pv.scope = Global;
                    }
                    Command::SetFunctions(sf) => unreachable!("{sf:?}"),
                    Command::PrintFunctions(pf) => unreachable!("{pf:?}"),
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
