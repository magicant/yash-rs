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
//! The export built-in (without the `-p` option) exports each of the specified
//! names to the environment, with optional values. If no names are given, or if
//! the `-p` option is given, the names and values of all exported variables are
//! displayed.
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
//! The operands are names of shell variables to be exported. Each name may
//! optionally be followed by `=` and a *value* to assign to the variable.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Errors
//!
//! When exporting a variable with a value, it is an error if the variable is
//! read-only.
//!
//! When printing variables, it is an error if an operand names a non-existing
//! variable.
//!
//! # Portability
//!
//! This built-in is part of the POSIX standard. Printing variables is portable
//! only when the `-p` option is used without operands.
//!
//! # Implementation notes
//!
//! The implementation of this built-in depends on that of the
//! [`typeset`](crate::typeset) built-in.

use crate::common::output;
use crate::common::report_error;
use crate::common::report_failure;
use crate::typeset::syntax::interpret;
use crate::typeset::syntax::parse;
use crate::typeset::syntax::OptionSpec;
use crate::typeset::syntax::PRINT_OPTION;
use crate::typeset::to_message;
use crate::typeset::Command;
use crate::typeset::PrintVariablesContext;
use crate::typeset::Scope::Global;
use crate::typeset::VariableAttr::Export;
use yash_env::option::State::On;
use yash_env::semantics::Field;
use yash_env::Env;

/// List of portable options applicable to the export built-in
pub static PORTABLE_OPTIONS: &[OptionSpec<'static>] = &[PRINT_OPTION];

/// Variable printing context for the export built-in
pub const PRINT_VARIABLES_CONTEXT: PrintVariablesContext<'static> = PrintVariablesContext {
    builtin_name: "export",
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
                match command.execute(env, &PRINT_VARIABLES_CONTEXT) {
                    Ok(result) => output(env, &result).await,
                    Err(errors) => report_failure(env, to_message(&errors)).await,
                }
            }
            Err(error) => report_error(env, &error).await,
        },
        Err(error) => report_error(env, &error).await,
    }
}
