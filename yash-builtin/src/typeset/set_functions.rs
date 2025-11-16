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

use super::*;
use std::rc::Rc;
use yash_env::function::FunctionSet;

impl SetFunctions {
    /// Executes the command.
    pub fn execute(self, functions: &mut FunctionSet) -> Result<String, Vec<ExecuteError>> {
        let mut errors = Vec::new();

        for name in self.functions {
            for &(attr, state) in &self.attrs {
                match (attr, state) {
                    (FunctionAttr::ReadOnly, State::On) => {
                        match functions.unset(&name.value) {
                            Ok(None) => {
                                errors.push(ExecuteError::ModifyUnsetFunction(name.clone()));
                            }

                            Err(_) => { /* The function is already read-only, do nothing. */ }

                            Ok(Some(mut function)) => {
                                Rc::make_mut(&mut function).read_only_location =
                                    Some(name.origin.clone());
                                functions
                                    .define(function)
                                    // The define function fails only when the set has a read-only
                                    // function with the same name. This is impossible because we
                                    // have just unset the function.
                                    .expect("unreachable error case");
                            }
                        }
                    }

                    (FunctionAttr::ReadOnly, State::Off) => match functions.get(&name.value) {
                        None => errors.push(ExecuteError::ModifyUnsetFunction(name.clone())),

                        Some(function) => {
                            if let Some(read_only_location) = function.read_only_location.clone() {
                                errors.push(ExecuteError::UndoReadOnlyFunction(
                                    UndoReadOnlyError {
                                        name: name.clone(),
                                        read_only_location,
                                    },
                                ));
                            }
                        }
                    },
                }
            }
        }

        if errors.is_empty() {
            Ok(String::new())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::ops::ControlFlow::Continue;
    use yash_env::Env;
    use yash_env::function::{Function, FunctionBody, FunctionBodyObject};
    use yash_env::semantics::ExitStatus;

    #[derive(Clone, Debug)]
    struct FunctionBodyStub;

    impl std::fmt::Display for FunctionBodyStub {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unreachable!()
        }
    }
    impl FunctionBody for FunctionBodyStub {
        async fn execute(&self, env: &mut Env) -> yash_env::semantics::Result {
            env.exit_status = ExitStatus::SUCCESS;
            Continue(())
        }
    }

    fn function_body_stub() -> Rc<dyn FunctionBodyObject> {
        Rc::new(FunctionBodyStub)
    }

    #[test]
    fn making_existing_functions_readonly() {
        let mut functions = FunctionSet::new();
        let foo = Function::new("foo", function_body_stub(), Location::dummy("foo location"));
        let bar = Function::new("bar", function_body_stub(), Location::dummy("bar location"))
            .make_read_only(Location::dummy("bar readonly location"));
        functions.define(foo.clone()).unwrap();
        functions.define(bar.clone()).unwrap();
        let sf = SetFunctions {
            functions: Field::dummies(["foo", "bar"]),
            attrs: vec![(FunctionAttr::ReadOnly, State::On)],
        };
        let foo_location = sf.functions[0].origin.clone();

        let result = sf.execute(&mut functions);

        assert_eq!(result, Ok("".to_string()));
        assert_eq!(
            **functions.get("foo").unwrap(),
            foo.make_read_only(foo_location),
        );
        // No change for bar because it is already read-only.
        assert_eq!(**functions.get("bar").unwrap(), bar);
    }

    #[test]
    fn unsetting_readonly_attribute_of_existing_functions() {
        let mut functions = FunctionSet::new();
        let foo = Function::new("foo", function_body_stub(), Location::dummy("foo location"));
        let bar_location = Location::dummy("bar readonly location");
        let bar = Function::new("bar", function_body_stub(), Location::dummy("bar location"))
            .make_read_only(bar_location.clone());
        functions.define(foo.clone()).unwrap();
        functions.define(bar.clone()).unwrap();
        let sf = SetFunctions {
            functions: Field::dummies(["foo", "bar"]),
            attrs: vec![(FunctionAttr::ReadOnly, State::Off)],
        };
        let arg_bar = sf.functions[1].clone();

        let errors = sf.execute(&mut functions).unwrap_err();

        assert_matches!(&errors[..], [ExecuteError::UndoReadOnlyFunction(error)] => {
            assert_eq!(error.name, arg_bar);
            assert_eq!(error.read_only_location, bar_location);
        });
        assert_eq!(**functions.get("foo").unwrap(), foo);
        assert_eq!(**functions.get("bar").unwrap(), bar);
    }

    #[test]
    fn making_non_existing_function_readonly() {
        let mut functions = FunctionSet::new();
        let sf = SetFunctions {
            functions: Field::dummies(["foo"]),
            attrs: vec![(FunctionAttr::ReadOnly, State::On)],
        };
        let arg_foo = sf.functions[0].clone();

        let errors = sf.execute(&mut functions).unwrap_err();

        assert_eq!(errors, [ExecuteError::ModifyUnsetFunction(arg_foo)]);
        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn unsetting_readonly_attribute_of_non_existing_functions() {
        let mut functions = FunctionSet::new();
        let sf = SetFunctions {
            functions: Field::dummies(["foo"]),
            attrs: vec![(FunctionAttr::ReadOnly, State::Off)],
        };
        let arg_foo = sf.functions[0].clone();

        let errors = sf.execute(&mut functions).unwrap_err();

        assert_eq!(errors, [ExecuteError::ModifyUnsetFunction(arg_foo)]);
        assert_eq!(functions.len(), 0);
    }
}
