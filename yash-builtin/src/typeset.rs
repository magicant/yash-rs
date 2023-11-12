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

//! Typeset built-in
//!
//! The built-in behaves differently depending on the arguments.
//!
//! # Defining variables
//!
//! If the `-p` (`--print`) or `-f` (`--functions`) option is not specified and
//! there are any operands, the built-in defines shell variables named by the
//! operands.
//!
//! Other options may be specified to set the scope and attributes of the
//! variables.
//!
//! ## Synopsis
//!
//! ```sh
//! typeset [-grx] [+rx] name[=value]...
//! ```
//!
//! ## Options
//!
//! By default, the built-in creates or updates variables in the current
//! context.  If the **`-g`** (**`--global`**) option is specified, the built-in
//! affects existing variables visible in the current scope (which may reside in
//! an outer context) or creates new variables in the base context. See the
//! documentation in [`yash_env::variable`] for details on the variable scope.
//!
//! The following options may be specified to set the attributes of the
//! variables:
//!
//! - **`-r`** (**`--readonly`**): Makes the variables read-only.
//! - **`-x`** (**`--export`**): Exports the variables to the environment.
//!
//! To remove the attributes, specify the corresponding option with a plus sign
//! (`+`) instead of a minus sign (`-`). For example, the following commands
//! stop exporting the variable `foo`:
//!
//! ```sh
//! typeset +x foo
//! typeset ++export foo
//! ```
//!
//! Note that the read-only attribute cannot be removed, so the `+r` option is
//! of no use.
//!
//! ## Operands
//!
//! Operands specify the names and values of the variables to be defined. If an
//! operand contains an equal sign (`=`), the operand is split into the name and
//! value at the first equal sign. The value is assigned to the variable named
//! by the name. Otherwise, the variable named by the operand is created without
//! a value unless it is already defined, in which case the existing value is
//! retained.
//!
//! If no operands are given, the built-in prints variables (see below).
//!
//! ## Standard output
//!
//! None.
//!
//! # Printing variables
//!
//! If the `-p` (`--print`) option is specified and the `-f` (`--functions`)
//! option is not specified, the built-in prints the attributes and values of
//! the variables named by the operands in the format that can be evaluated as
//! shell code to recreate the variables.
//! <!-- TODO: link to the eval built-in -->
//! If there are no operands and the `-f` (`--functions`) option is not
//! specified, the built-in prints all shell variables in the same format.
//!
//! ## Synopsis
//!
//! ```sh
//! typeset -p [-grx] [+rx] [name...]
//! ```
//!
//! ```sh
//! typeset [-grx] [+rx]
//! ```
//!
//! ## Options
//!
//! The **`-p`** (**`--print`**) option must be specified to print variables
//! when there are any operands. Otherwise, the built-in defines variables. The
//! option may be omitted if there are no operands.
//!
//! By default, the built-in prints variables in the current context. If the
//! **`-g`** (**`--global`**) option is specified, the built-in prints variables
//! visible in the current scope (which may reside in an outer context).
//!
//! The following options may be specified to select which variables to print.
//! Variables that do not match the selection criteria are ignored.
//!
//! - **`-r`** (**`--readonly`**): Prints read-only variables.
//! - **`-x`** (**`--export`**): Prints exported variables.
//!
//! If these options are negated by prefixing a plus sign (`+`) instead of a
//! minus sign (`-`), the built-in prints variables that do not have the
//! corresponding attribute.
//!
//! ## Operands
//!
//! Operands specify the names of the variables to be printed. If no operands
//! are given, the built-in prints all variables that match the selection
//! criteria.
//!
//! ## Standard output
//!
//! A command string that invokes the typeset built-in to recreate the variable
//! is printed for each variable.
//!
//! Note that evaluating the printed commands in the current context may fail if
//! variables are read-only since the read-only variables cannot be assigned
//! values.
//!
//! # Modifying functions
//!
//! If the `-f` (`--functions`) option is specified, the `-p` (`--print`) option
//! is not specified, and there are any operands, the built-in modifies the
//! attributes of shell functions named by the operands.
//!
//! ## Synopsis
//!
//! ```sh
//! typeset -f [-r] [+r] name...
//! ```
//!
//! ## Options
//!
//! The **`-f`** (**`--functions`**) option is required to modify functions.
//! Otherwise, the built-in defines variables.
//!
//! The **`-r`** (**`--readonly`**) option makes the functions read-only. If the
//! option is not specified, the built-in does nothing.
//!
//! The built-in accepts the `+r` (`++readonly`) option, but it is of no use
//! since the read-only attribute cannot be removed.
//!
//! ## Operands
//!
//! Operands specify the names of the functions to be modified. If no operands
//! are given, the built-in prints functions (see below).
//!
//! Note that the built-in operates on existing shell functions only. It cannot
//! create new functions or change the contents of existing functions.
//!
//! ## Standard output
//!
//! None.
//!
//! # Printing functions
//!
//! If the `-f` (`--functions`) and `-p` (`--print`) options are specified, the
//! built-in prints the attributes and definitions of the shell functions named
//! by the operands in the format that can be evaluated as shell code to
//! recreate the functions.
//! <!-- TODO: link to the eval built-in -->
//! If there are no operands and the `-f` (`--functions`) option is specified,
//! the built-in prints all shell functions in the same format.
//!
//! ## Synopsis
//!
//! ```sh
//! typeset -fp [-r] [+r] [name...]
//! ```
//!
//! ```sh
//! typeset -f [-r] [+r]
//! ```
//!
//! ## Options
//!
//! The the **`-f`** (**`--functions`**) and **`-p`** (**`--print`**) options
//! must be specified to print functions when there are any operands. Otherwise,
//! the built-in modifies functions. The `-p` (`--print`) option may be omitted
//! if there are no operands.
//!
//! The **`-r`** (**`--readonly`**) option can be specified to limit the output
//! to read-only functions. If this option is negated as `+r` (`++readonly`),
//! the built-in prints functions that are not read-only. If the option is not
//! specified, the built-in prints all functions.
//!
//! ## Operands
//!
//! Operands specify the names of the functions to be printed. If no operands
//! are given, the built-in prints all functions that match the selection
//! criteria.
//!
//! ## Standard output
//!
//! A command string of a function definition command is printed for each
//! function, which may be followed by an invocation of the typeset built-in to
//! set the attributes of the function.
//!
//! Note that evaluating the printed commands in the current shell environment
//! may fail if functions are read-only since the read-only functions cannot be
//! redefined.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Errors
//!
//! The read-only attribute cannot be removed from a variable or function. If a
//! variable is already read-only, you cannot assign a value to it.
//!
//! When printing variables or functions, it is an error if an operand names a
//! non-existing variable or function.
//!
//! # Portability
//!
//! The typeset built-in is not specified by POSIX and many shells implement it
//! differently. This implementation is based on common characteristics seen in
//! other shells, but it is not fully compatible with any of them.
//!
//! Some implementations allow operating on variables and functions at the same
//! time. This implementation does not.
//!
//! This implementation requires the `-g` (`--global`) option to print variables
//! defined in outer contexts. Other implementations may print such variables by
//! default.
//!
//! This implementation allows hiding a read-only variable defined in an outer
//! context by introducing a variable with the same name in the current context.
//! This may not be allowed in other implementations.
//!
//! Historical versions of yash used to perform assignments when operands of the
//! form `name=value` are given even if the `-p` option is specified. This
//! implementation regards such usage as an error.
//!
//! Historical versions of yash used the `-X` (`--unexport`) option to negate
//! the `-x` (`--export`) option. This is now deprecated because its behavior is
//! incompatible with other implementations. Use the `+x` (`++export`) option
//! instead.
//!
//! # Implementation notes
//!
//! TBD

use crate::common::{output, report_error, report_failure};
use thiserror::Error;
use yash_env::function::Function;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::variable::{Value, Variable};
use yash_env::Env;
use yash_syntax::source::pretty::{Annotation, AnnotationType, Message, MessageBase};
use yash_syntax::source::Location;

mod print_functions;
mod print_variables;
mod set_functions;
mod set_variables;
pub mod syntax;

/// Attribute that can be set on a variable
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum VariableAttr {
    /// The variable is read-only.
    ReadOnly,
    /// The variable is exported to the environment.
    Export,
}

impl VariableAttr {
    /// Tests if the attribute is set on a variable
    #[must_use]
    pub fn test(&self, var: &Variable) -> State {
        let is_on = match self {
            VariableAttr::ReadOnly => var.is_read_only(),
            VariableAttr::Export => var.is_exported,
        };
        State::from(is_on)
    }
}

/// Scope in which a variable is defined or selected
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Scope {
    /// Operates on global scope.
    ///
    /// When defining variables: If an existing variable is visible in the
    /// current scope, the variable is updated. Otherwise, a new variable is
    /// created in the base context.
    ///
    /// When printing variables: All visible variables are printed.
    Global,

    /// Operates on local scope.
    ///
    /// When defining variables: The variable is defined in the local context of
    /// the current function.
    ///
    /// When printing variables: Only variables defined in the local context of
    /// the current function are printed.
    Local,
}

/// Set of information to define variables
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetVariables {
    /// Names and optional values of the variables to be defined
    pub variables: Vec<Field>,
    /// Attributes to be set on the variables
    pub attrs: Vec<(VariableAttr, State)>,
    /// Scope in which the variables are defined
    pub scope: Scope,
}

/// Set of information to print variables
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintVariables {
    /// Names of the variables to be printed
    ///
    /// If empty, all variables are printed.
    pub variables: Vec<Field>,
    /// Attributes to select the variables to be printed
    pub attrs: Vec<(VariableAttr, State)>,
    /// Scope in which the variables are printed
    pub scope: Scope,
}

/// Attribute that can be set on a function
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FunctionAttr {
    /// The function is read-only.
    ReadOnly,
}

impl FunctionAttr {
    /// Tests if the attribute is set on a function.
    #[must_use]
    fn test(&self, function: &Function) -> State {
        let is_on = match self {
            Self::ReadOnly => function.is_read_only(),
        };
        State::from(is_on)
    }
}

/// Set of information to modify functions
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetFunctions {
    /// Names of the functions to be modified
    pub functions: Vec<Field>,
    /// Attributes to be set on the functions
    pub attrs: Vec<(FunctionAttr, State)>,
}

/// Set of information to print functions
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintFunctions {
    /// Names of the functions to be printed
    ///
    /// If empty, all functions are printed.
    pub functions: Vec<Field>,
    /// Attributes to select the functions to be printed
    pub attrs: Vec<(FunctionAttr, State)>,
}

/// Set of information that defines the behavior of a single invocation of the
/// typeset built-in
///
/// The [`syntax::interpret`] function returns a value of this type after
/// parsing the arguments. Call the [`execute`](Self::execute) method to perform
/// the actual operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    SetVariables(SetVariables),
    PrintVariables(PrintVariables),
    SetFunctions(SetFunctions),
    PrintFunctions(PrintFunctions),
}

impl From<SetVariables> for Command {
    fn from(v: SetVariables) -> Self {
        Self::SetVariables(v)
    }
}

impl From<PrintVariables> for Command {
    fn from(v: PrintVariables) -> Self {
        Self::PrintVariables(v)
    }
}

impl From<SetFunctions> for Command {
    fn from(v: SetFunctions) -> Self {
        Self::SetFunctions(v)
    }
}

impl From<PrintFunctions> for Command {
    fn from(v: PrintFunctions) -> Self {
        Self::PrintFunctions(v)
    }
}

impl Command {
    /// Executes the command (except for actual printing).
    ///
    /// This method updates the shell environment according to the command.
    /// If there are no errors, the method returns a string that should be
    /// printed to the standard output.
    /// Otherwise, the method returns a non-empty vector of errors.
    pub fn execute(self, env: &mut Env) -> Result<String, Vec<ExecuteError>> {
        match self {
            Self::SetVariables(command) => command.execute(env),
            Self::PrintVariables(command) => command.execute(&env.variables),
            Self::SetFunctions(command) => command.execute(&mut env.functions),
            Self::PrintFunctions(command) => command.execute(&env.functions),
        }
    }
}

/// Error returned on assigning to a read-only variable
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot assign to read-only variable {name:?}")]
pub struct AssignReadOnlyError {
    /// Name of the read-only variable
    pub name: String,
    /// Value that was being assigned
    pub new_value: Value,
    /// Location where the variable was tried to be assigned
    pub assigned_location: Location,
    /// Location where the variable was made read-only
    pub read_only_location: Location,
}

impl From<AssignReadOnlyError> for yash_env::variable::AssignError {
    fn from(e: AssignReadOnlyError) -> Self {
        Self {
            new_value: e.new_value,
            assigned_location: Some(e.assigned_location),
            read_only_location: e.read_only_location,
        }
    }
}

/// This conversion is available only when the optional `yash-semantics`
/// dependency is enabled.
#[cfg(feature = "yash-semantics")]
impl From<AssignReadOnlyError> for yash_semantics::expansion::AssignReadOnlyError {
    fn from(e: AssignReadOnlyError) -> Self {
        Self {
            name: e.name,
            new_value: e.new_value,
            read_only_location: e.read_only_location,
        }
    }
}

/// Error that occurs when trying to cancel the read-only attribute of a
/// variable or function
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("cannot cancel read-only-ness of {name}")]
pub struct UndoReadOnlyError {
    /// Name of the variable or function
    pub name: Field,
    /// Location where the variable or function was made read-only
    pub read_only_location: Location,
}

/// Error that can occur during the execution of the typeset built-in
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ExecuteError {
    /// Assigning to a read-only variable
    AssignReadOnlyVariable(#[from] AssignReadOnlyError),
    /// Cancelling the read-only attribute of a variable
    UndoReadOnlyVariable(UndoReadOnlyError),
    /// Cancelling the read-only attribute of a function
    UndoReadOnlyFunction(UndoReadOnlyError),
    /// Modifying a non-existing function
    ModifyUnsetFunction(Field),
    /// Printing a non-existing variable
    PrintUnsetVariable(Field),
    /// Printing a non-existing function
    PrintUnsetFunction(Field),
}

impl MessageBase for ExecuteError {
    fn message_title(&self) -> std::borrow::Cow<str> {
        match self {
            Self::AssignReadOnlyVariable(_) => "cannot assign to read-only variable",
            Self::UndoReadOnlyVariable(_) => "cannot cancel read-only-ness of variable",
            Self::UndoReadOnlyFunction(_) => "cannot cancel read-only-ness of function",
            Self::ModifyUnsetFunction(_) => "cannot modify non-existing function",
            Self::PrintUnsetVariable(_) => "cannot print non-existing variable",
            Self::PrintUnsetFunction(_) => "cannot print non-existing function",
        }
        .into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let (message, location) = match self {
            Self::AssignReadOnlyVariable(error) => (error.to_string(), &error.assigned_location),
            Self::UndoReadOnlyVariable(error) => (
                format!("read-only variable `{}`", error.name),
                &error.name.origin,
            ),
            Self::UndoReadOnlyFunction(error) => (
                format!("read-only function `{}`", error.name),
                &error.name.origin,
            ),
            Self::PrintUnsetVariable(field) => {
                (format!("non-existing variable `{field}`"), &field.origin)
            }
            Self::ModifyUnsetFunction(field) | Self::PrintUnsetFunction(field) => {
                (format!("non-existing function `{field}`"), &field.origin)
            }
        };
        Annotation::new(AnnotationType::Error, message.into(), location)
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        match self {
            Self::AssignReadOnlyVariable(error) => {
                results.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    "the variable was made read-only here".into(),
                    &error.read_only_location,
                )))
            }

            Self::UndoReadOnlyVariable(error) => results.extend(std::iter::once(Annotation::new(
                AnnotationType::Info,
                "the variable was made read-only here".into(),
                &error.read_only_location,
            ))),

            Self::UndoReadOnlyFunction(error) => results.extend(std::iter::once(Annotation::new(
                AnnotationType::Info,
                "the function was made read-only here".into(),
                &error.read_only_location,
            ))),

            Self::ModifyUnsetFunction(_)
            | Self::PrintUnsetVariable(_)
            | Self::PrintUnsetFunction(_) => {}
        }
    }
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message_title().fmt(f)
    }
}

/// Converts a non-empty slice of errors to a message.
///
/// The first error's title is used as the message title. The other errors are
/// added as annotations.
#[must_use]
fn to_message(errors: &[ExecuteError]) -> Message {
    let mut message = Message::from(&errors[0]);
    let other_errors = errors[1..].iter().map(ExecuteError::main_annotation);
    message.annotations.extend(other_errors);
    message
}

/// Entry point of the typeset built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> yash_env::builtin::Result {
    match syntax::parse(syntax::ALL_OPTIONS, args) {
        Ok((options, operands)) => match syntax::interpret(options, operands) {
            Ok(command) => match command.execute(env) {
                Ok(result) => output(env, &result).await,
                Err(errors) => report_failure(env, to_message(&errors)).await,
            },
            Err(error) => report_error(env, &error).await,
        },
        Err(error) => report_error(env, &error).await,
    }
}
