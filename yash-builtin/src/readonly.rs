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
//! This module implements the [`readonly` built-in], which makes variables or
//! functions read-only.
//!
//! [`readonly` built-in]: https://magicant.github.io/yash-rs/builtins/readonly.html
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
use crate::common::report::{merge_reports, report_error, report_failure};
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
                    Err(errors) => report_failure(env, merge_reports(&errors).unwrap()).await,
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
