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

//! Implementations of function definition semantics.

use crate::command::Command;
use crate::expansion::expand_word;
use crate::expansion::Field;
use crate::Handle;
use std::ops::ControlFlow::Continue;
use std::rc::Rc;
use yash_env::function::DefineError;
use yash_env::function::Function;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::syntax;

/// Executes the function definition command.
///
/// First, the function name is [expanded](expand_word). If the expansion fails,
/// the execution ends with a non-zero exit status. Next, the environment is
/// examined for an existing function having the same name.  If there is such a
/// function that is read-only, the execution ends with a non-zero exit status.
/// Finally, the function definition is inserted into the environment, and the
/// execution ends with an exit status of zero.
///
/// The `ErrExit` shell option is [applied](Env::apply_errexit) on error.
impl Command for syntax::FunctionDefinition {
    async fn execute(&self, env: &mut Env) -> Result {
        define_function(env, self).await?;
        env.apply_errexit()
    }
}

async fn define_function(env: &mut Env, def: &syntax::FunctionDefinition) -> Result {
    // Expand the function name
    let Field {
        value: name,
        origin,
    } = match expand_word(env, &def.name).await {
        Ok((field, _exit_status)) => field,
        Err(error) => return error.handle(env).await,
    };

    // Define the function
    let function = Function::new(name, Rc::clone(&def.body), origin);
    match env.functions.define(function) {
        Ok(_) => {
            env.exit_status = ExitStatus::SUCCESS;
        }
        Err(error) => {
            report_define_error(env, &error).await;
            env.exit_status = ExitStatus::ERROR;
        }
    }
    Continue(())
}

/// Reports a function definition error.
///
/// This function assumes `error.existing.read_only_location.is_some()`.
async fn report_define_error(env: &mut Env, error: &DefineError) {
    let message = Message {
        r#type: AnnotationType::Error,
        title: error.to_string().into(),
        annotations: vec![
            Annotation::new(
                AnnotationType::Error,
                "failed function redefinition".into(),
                &error.new.origin,
            ),
            Annotation::new(
                AnnotationType::Info,
                "existing function was defined here".into(),
                &error.existing.origin,
            ),
            Annotation::new(
                AnnotationType::Info,
                "existing function was made read-only here".into(),
                error.existing.read_only_location.as_ref().unwrap(),
            ),
        ],
        footers: vec![],
    };

    yash_env::io::print_message(env, message).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use yash_env::option::On;
    use yash_env::option::Option::ErrExit;
    use yash_env::semantics::Divert;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::assert_stderr;
    use yash_syntax::source::Location;

    #[test]
    fn function_definition_new() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus::ERROR;
        let definition = syntax::FunctionDefinition {
            has_keyword: false,
            name: "foo".parse().unwrap(),
            body: Rc::new("{ :; }".parse().unwrap()),
        };

        let result = definition.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.functions.len(), 1);
        let function = env.functions.get("foo").unwrap();
        assert_eq!(function.name, "foo");
        assert_eq!(function.origin, definition.name.location);
        assert_eq!(function.body, definition.body);
        assert_eq!(function.read_only_location, None);
    }

    #[test]
    fn function_definition_overwrite() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus::ERROR;
        let function = Function {
            name: "foo".to_string(),
            body: Rc::new("{ :; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            read_only_location: None,
        };
        env.functions.define(function).unwrap();
        let definition = syntax::FunctionDefinition {
            has_keyword: false,
            name: "foo".parse().unwrap(),
            body: Rc::new("( :; )".parse().unwrap()),
        };

        let result = definition.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.functions.len(), 1);
        let function = env.functions.get("foo").unwrap();
        assert_eq!(function.name, "foo");
        assert_eq!(function.origin, definition.name.location);
        assert_eq!(function.body, definition.body);
        assert_eq!(function.read_only_location, None);
    }

    #[test]
    fn function_definition_read_only() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let function = Rc::new(Function {
            name: "foo".to_string(),
            body: Rc::new("{ :; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            read_only_location: Some(Location::dummy("readonly")),
        });
        env.functions.define(Rc::clone(&function)).unwrap();
        let definition = syntax::FunctionDefinition {
            has_keyword: false,
            name: "foo".parse().unwrap(),
            body: Rc::new("( :; )".parse().unwrap()),
        };

        let result = definition.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
        assert_eq!(env.functions.len(), 1);
        assert_eq!(env.functions.get("foo").unwrap(), &function);
        assert_stderr(&state, |stderr| {
            assert!(
                stderr.contains("foo"),
                "error message should contain function name: {stderr:?}"
            )
        });
    }

    #[test]
    fn function_definition_name_expansion() {
        let mut env = Env::new_virtual();
        let definition = syntax::FunctionDefinition {
            has_keyword: false,
            name: r"\a".parse().unwrap(),
            body: Rc::new("{ :; }".parse().unwrap()),
        };

        let result = definition.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        let names: Vec<&str> = env.functions.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a"]);
    }

    #[test]
    fn errexit_in_function_definition() {
        let mut env = Env::new_virtual();
        let function = Function {
            name: "foo".to_string(),
            body: Rc::new("{ :; }".parse().unwrap()),
            origin: Location::dummy("dummy"),
            read_only_location: Some(Location::dummy("readonly")),
        };
        env.functions.define(function).unwrap();
        let definition = syntax::FunctionDefinition {
            has_keyword: false,
            name: "foo".parse().unwrap(),
            body: Rc::new("( :; )".parse().unwrap()),
        };
        env.options.set(ErrExit, On);

        let result = definition.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(None)));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
    }
}
