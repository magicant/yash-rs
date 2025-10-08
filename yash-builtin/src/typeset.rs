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
//! This module implements the [`typeset` built-in], which defines variables or
//! functions with specified attributes.
//!
//! [`typeset` built-in]: https://magicant.github.io/yash-rs/builtins/typeset.html
//!
//! # Implementation notes
//!
//! The implementation of this built-in is also used by the
//! [`export`](crate::export) and [`readonly`](crate::readonly) built-ins.
//! Functions that are common to these built-ins and the typeset built-in are
//! parameterized to support the different behaviors of the built-ins. By
//! customizing the contents of [`Command`] and the [`PrintContext`] passed to
//! [`Command::execute`], you can even implement a new built-in that behaves
//! differently from all of them.

use self::syntax::OptionSpec;
use crate::common::output;
use crate::common::report::{merge_reports, report_error, report_failure};
use thiserror::Error;
use yash_env::Env;
use yash_env::function::Function;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::variable::{Value, Variable};
use yash_syntax::source::Location;
#[allow(deprecated)]
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_syntax::source::pretty::{Report, ReportType, Snippet, Span, SpanRole, add_span};

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

/// Set of information used when printing variables or functions
///
/// [`PrintVariables::execute`] and [`PrintFunctions::execute`] print a list of
/// commands that invoke a built-in to recreate variables and functions,
/// respectively. This context is used to control the details of the commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrintContext<'a> {
    /// Name of the built-in printed as part of the commands to recreate the
    /// variables or functions
    pub builtin_name: &'a str,

    /// Whether the command that invokes the built-in should always be printed
    ///
    /// The typeset built-in does not itself modify the attributes of variables
    /// or functions when invoked simply with a name operand. If a separate
    /// array assignment or function definition command is sufficient to
    /// reproduce an array variable or function, the command that invokes the
    /// typeset built-in may be omitted. This field indicates whether the
    /// command should always be printed regardless of the attributes of the
    /// variables or functions.
    ///
    /// This field should be false for the typeset built-in to allow omitting,
    /// but it should be true for the export and readonly built-ins to force
    /// printing as they always modify the attributes.
    pub builtin_is_significant: bool,

    /// Options that may be printed for the built-in
    ///
    /// When printing a command that invokes the built-in, the command may
    /// include options that appear in this slice to re-set the attributes of
    /// the variables or functions.
    pub options_allowed: &'a [OptionSpec<'a>],
}

/// Printing context for the typeset built-in
pub const PRINT_CONTEXT: PrintContext<'static> = PrintContext {
    builtin_name: "typeset",
    builtin_is_significant: false,
    options_allowed: self::syntax::ALL_OPTIONS,
};

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
    pub fn execute(
        self,
        env: &mut Env,
        print_context: &PrintContext,
    ) -> Result<String, Vec<ExecuteError>> {
        match self {
            Self::SetVariables(command) => command.execute(env),
            Self::PrintVariables(command) => command.execute(&env.variables, print_context),
            Self::SetFunctions(command) => command.execute(&mut env.functions),
            Self::PrintFunctions(command) => command.execute(&env.functions, print_context),
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
            vacancy: None,
        }
    }
}

impl AssignReadOnlyError {
    /// Converts the error to a report.
    #[must_use]
    fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "error assigning to variable".into();
        report.snippets =
            Snippet::with_primary_span(&self.assigned_location, self.to_string().into());
        add_span(
            &self.read_only_location.code,
            Span {
                range: self.read_only_location.byte_range(),
                role: SpanRole::Supplementary {
                    label: "the variable was made read-only here".into(),
                },
            },
            &mut report.snippets,
        );
        report
    }
}

impl<'a> From<&'a AssignReadOnlyError> for Report<'a> {
    #[inline]
    fn from(error: &'a AssignReadOnlyError) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for AssignReadOnlyError {
    fn message_title(&self) -> std::borrow::Cow<'_, str> {
        "cannot assign to read-only variable".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            self.to_string().into(),
            &self.assigned_location,
        )
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        // TODO Use extend_one
        results.extend(std::iter::once(Annotation::new(
            AnnotationType::Info,
            "the variable was made read-only here".into(),
            &self.read_only_location,
        )))
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

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AssignReadOnlyVariable(e) => e.fmt(f),
            Self::UndoReadOnlyVariable(e) => {
                write!(f, "cannot cancel read-only-ness of variable `{}`", e.name)
            }
            Self::UndoReadOnlyFunction(e) => {
                write!(f, "cannot cancel read-only-ness of function `{}`", e.name)
            }
            Self::ModifyUnsetFunction(field) => {
                write!(f, "cannot modify non-existing function `{}`", field)
            }
            Self::PrintUnsetVariable(field) => {
                write!(f, "cannot print non-existing variable `{}`", field)
            }
            Self::PrintUnsetFunction(field) => {
                write!(f, "cannot print non-existing function `{}`", field)
            }
        }
    }
}

impl ExecuteError {
    /// Converts the error to a report.
    #[must_use]
    fn to_report(&self) -> Report<'_> {
        let (title, location, label) = match self {
            Self::AssignReadOnlyVariable(error) => return error.to_report(),
            Self::UndoReadOnlyVariable(error) => (
                "cannot cancel read-only-ness of variable",
                &error.name.origin,
                format!("read-only variable `{}`", error.name.value).into(),
            ),
            Self::UndoReadOnlyFunction(error) => (
                "cannot cancel read-only-ness of function",
                &error.name.origin,
                format!("read-only function `{}`", error.name.value).into(),
            ),
            Self::ModifyUnsetFunction(field) => (
                "cannot modify non-existing function",
                &field.origin,
                format!("non-existing function `{field}`").into(),
            ),
            Self::PrintUnsetVariable(field) => (
                "cannot print non-existing variable",
                &field.origin,
                format!("non-existing variable `{field}`").into(),
            ),
            Self::PrintUnsetFunction(field) => (
                "cannot print non-existing function",
                &field.origin,
                format!("non-existing function `{field}`").into(),
            ),
        };

        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = title.into();
        report.snippets = Snippet::with_primary_span(location, label);
        match self {
            Self::UndoReadOnlyVariable(error) => add_span(
                &error.read_only_location.code,
                Span {
                    range: error.read_only_location.byte_range(),
                    role: SpanRole::Supplementary {
                        label: "the variable was made read-only here".into(),
                    },
                },
                &mut report.snippets,
            ),
            Self::UndoReadOnlyFunction(error) => add_span(
                &error.read_only_location.code,
                Span {
                    range: error.read_only_location.byte_range(),
                    role: SpanRole::Supplementary {
                        label: "the function was made read-only here".into(),
                    },
                },
                &mut report.snippets,
            ),
            _ => { /* No additional spans */ }
        }
        report
    }
}

impl<'a> From<&'a ExecuteError> for Report<'a> {
    #[inline]
    fn from(error: &'a ExecuteError) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for ExecuteError {
    fn message_title(&self) -> std::borrow::Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let (message, location) = match self {
            Self::AssignReadOnlyVariable(error) => return error.main_annotation(),
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
            Self::AssignReadOnlyVariable(error) => error.additional_annotations(results),

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

/// Entry point of the typeset built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> yash_env::builtin::Result {
    match syntax::parse(syntax::ALL_OPTIONS, args) {
        Ok((options, operands)) => match syntax::interpret(options, operands) {
            Ok(command) => match command.execute(env, &PRINT_CONTEXT) {
                Ok(result) => output(env, &result).await,
                Err(errors) => report_failure(env, merge_reports(&errors).unwrap()).await,
            },
            Err(error) => report_error(env, &error).await,
        },
        Err(error) => report_error(env, &error).await,
    }
}
