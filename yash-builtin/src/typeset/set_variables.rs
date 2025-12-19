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
use yash_env::variable::Value;

impl From<Scope> for yash_env::variable::Scope {
    fn from(value: Scope) -> Self {
        match value {
            Scope::Local => Self::Local,
            Scope::Global => Self::Global,
        }
    }
}

impl SetVariables {
    /// Executes the command.
    pub fn execute<S>(self, env: &mut Env<S>) -> Result<String, Vec<ExecuteError>> {
        let mut errors = Vec::new();

        'field: for mut field in self.variables {
            // Split the field into the name and the value.
            let mut value_to_assign = None;
            if let Some((name, value)) = field.value.split_once('=') {
                value_to_assign = Some(Value::scalar(value));

                // Modify the field value so that it contains only the name.
                field.value.truncate(name.len());
            }

            let mut variable = env.get_or_create_variable(&field.value, self.scope.into());

            // Assign the value to the variable.
            if let Some(value) = value_to_assign {
                if let Err(error) = variable.assign(value, field.origin.clone()) {
                    errors.push(ExecuteError::AssignReadOnlyVariable(AssignReadOnlyError {
                        name: field.value,
                        new_value: error.new_value,
                        assigned_location: error.assigned_location.unwrap(),
                        read_only_location: error.read_only_location,
                    }));
                    continue;
                }
            }

            // Apply the attributes to the variable.
            for &(attr, state) in &self.attrs {
                match (attr, state) {
                    (VariableAttr::ReadOnly, State::On) => {
                        variable.make_read_only(field.origin.clone())
                    }
                    (VariableAttr::ReadOnly, State::Off) => {
                        if let Some(read_only_location) = variable.read_only_location.clone() {
                            errors.push(ExecuteError::UndoReadOnlyVariable(UndoReadOnlyError {
                                name: field,
                                read_only_location,
                            }));
                            continue 'field;
                        }
                    }
                    (VariableAttr::Export, State::On) => variable.export(true),
                    (VariableAttr::Export, State::Off) => variable.export(false),
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
    use yash_env::option::Option::AllExport;
    use yash_env::source::Location;
    use yash_env::variable::{Context, Variable};

    #[test]
    fn setting_local_variables() {
        let mut outer = Env::new_virtual();
        let mut inner = outer.push_context(Context::default());
        let baz_location = Location::dummy("baz assigned");
        let mut baz = inner.get_or_create_variable("baz", Scope::Local.into());
        baz.assign("BAZ", baz_location.clone()).unwrap();
        let sv = SetVariables {
            variables: Field::dummies(["foo=FOO", "bar", "baz"]),
            attrs: vec![],
            scope: Scope::Local,
        };
        let foo_location = sv.variables[0].origin.clone();

        let result = sv.execute(&mut inner);

        assert_eq!(result, Ok("".to_string()));
        // $foo now has the value assigned by `execute`
        let foo = inner.variables.get("foo").unwrap();
        assert_eq!(foo.value, Some(Value::scalar("FOO")));
        assert_eq!(foo.last_assigned_location, Some(foo_location));
        assert_eq!(foo.read_only_location, None);
        // $bar now exists but has no value
        let bar = inner.variables.get("bar").unwrap();
        assert_eq!(bar.value, None);
        assert_eq!(bar.last_assigned_location, None);
        assert_eq!(bar.read_only_location, None);
        // $baz retains the previous value since `execute` assigned nothing
        let baz = inner.variables.get("baz").unwrap();
        assert_eq!(baz.value, Some(Value::scalar("BAZ")));
        assert_eq!(baz.last_assigned_location, Some(baz_location));
        assert_eq!(baz.read_only_location, None);
        // All the variables are local, so they aren't visible from the outer context
        Env::pop_context(inner);
        assert_eq!(outer.variables.get("foo"), None);
        assert_eq!(outer.variables.get("bar"), None);
        assert_eq!(outer.variables.get("baz"), None);
    }

    #[test]
    fn setting_global_variables() {
        let mut outer = Env::new_virtual();
        let baz_location = Location::dummy("assign");
        let mut baz = outer.get_or_create_variable("baz", Scope::Global.into());
        baz.assign("BAZ", baz_location.clone()).unwrap();
        let mut inner = outer.push_context(Context::default());
        let sv = SetVariables {
            variables: Field::dummies(["foo=FOO", "bar", "baz"]),
            attrs: vec![],
            scope: Scope::Global,
        };
        let foo_location = sv.variables[0].origin.clone();

        let result = sv.execute(&mut inner);

        assert_eq!(result, Ok("".to_string()));
        // All the variables are global, so they are visible from the outer context
        Env::pop_context(inner);
        // $foo now has the value assigned by `execute`
        let foo = outer.variables.get("foo").unwrap();
        assert_eq!(foo.value, Some(Value::scalar("FOO")));
        assert_eq!(foo.last_assigned_location, Some(foo_location));
        assert_eq!(foo.read_only_location, None);
        // $bar now exists but has no value
        let bar = outer.variables.get("bar").unwrap();
        assert_eq!(bar.value, None);
        assert_eq!(bar.last_assigned_location, None);
        assert_eq!(bar.read_only_location, None);
        // $baz retains the previous value since `execute` assigned nothing
        let baz = outer.variables.get("baz").unwrap();
        assert_eq!(baz.value, Some(Value::scalar("BAZ")));
        assert_eq!(baz.last_assigned_location, Some(baz_location));
        assert_eq!(baz.read_only_location, None);
    }

    #[test]
    fn setting_variables_readonly() {
        let mut env = Env::new_virtual();
        let sv = SetVariables {
            variables: Field::dummies(["foo", "bar=BAR"]),
            attrs: vec![(VariableAttr::ReadOnly, State::On)],
            scope: Scope::Local,
        };
        let foo_location = sv.variables[0].origin.clone();
        let bar_location = sv.variables[1].origin.clone();

        let result = sv.execute(&mut env);

        assert_eq!(result, Ok("".to_string()));
        let foo = env.variables.get("foo").unwrap();
        assert_eq!(foo.value, None);
        assert_eq!(foo.last_assigned_location.as_ref(), None);
        assert_eq!(foo.read_only_location.as_ref(), Some(&foo_location));
        let bar = env.variables.get("bar").unwrap();
        assert_eq!(bar.value, Some(Value::scalar("BAR")));
        assert_eq!(bar.last_assigned_location.as_ref(), Some(&bar_location));
        assert_eq!(bar.read_only_location.as_ref(), Some(&bar_location));
    }

    #[test]
    fn exporting_variables() {
        let mut env = Env::new_virtual();
        let sv = SetVariables {
            variables: Field::dummies(["foo", "bar=BAR"]),
            attrs: vec![(VariableAttr::Export, State::On)],
            scope: Scope::Local,
        };

        let result = sv.execute(&mut env);

        assert_eq!(result, Ok("".to_string()));
        let foo = env.variables.get("foo").unwrap();
        assert_eq!(foo.value, None);
        assert!(foo.is_exported);
        let bar = env.variables.get("bar").unwrap();
        assert_eq!(bar.value, Some(Value::scalar("BAR")));
        assert!(bar.is_exported);
    }

    #[test]
    fn cancelling_exportation() {
        let mut env = Env::new_virtual();
        let mut var = env.get_or_create_variable("bar", Scope::Global.into());
        var.assign("BAR", None).unwrap();
        var.export(true);
        let sv = SetVariables {
            variables: Field::dummies(["foo", "bar=NEW_BAR"]),
            attrs: vec![(VariableAttr::Export, State::Off)],
            scope: Scope::Local,
        };

        let result = sv.execute(&mut env);

        assert_eq!(result, Ok("".to_string()));
        let foo = env.variables.get("foo").unwrap();
        assert_eq!(foo.value, None);
        assert!(!foo.is_exported);
        let bar = env.variables.get("bar").unwrap();
        assert_eq!(bar.value, Some(Value::scalar("NEW_BAR")));
        assert!(!bar.is_exported);
    }

    #[test]
    fn exportation_with_allexport_option() {
        let mut env = Env::new_virtual();
        env.options.set(AllExport, State::On);

        let sv = SetVariables {
            variables: Field::dummies(["foo=FOO"]),
            attrs: vec![],
            scope: Scope::Global,
        };
        let result = sv.execute(&mut env);
        assert_eq!(result, Ok("".to_string()));
        assert!(env.variables.get("foo").unwrap().is_exported);

        let sv = SetVariables {
            variables: Field::dummies(["foo=BAR"]),
            attrs: vec![(VariableAttr::Export, State::Off)],
            scope: Scope::Global,
        };
        let result = sv.execute(&mut env);
        assert_eq!(result, Ok("".to_string()));
        assert!(!env.variables.get("foo").unwrap().is_exported);
    }

    #[test]
    fn unsetting_readonly_attribute() {
        let mut env = Env::new_virtual();
        let ro_location = Location::dummy("assign readonly");
        let mut ro = env.get_or_create_variable("ro", Scope::Global.into());
        ro.assign("readonly value", ro_location.clone()).unwrap();
        ro.make_read_only(ro_location.clone());
        let ro = ro.clone();
        let w_location = Location::dummy("assign writable");
        let mut w = env.get_or_create_variable("w", Scope::Global.into());
        w.assign("writable value", w_location).unwrap();
        let w = w.clone();
        let sv = SetVariables {
            variables: Field::dummies(["ro", "w=foo"]),
            attrs: vec![(VariableAttr::ReadOnly, State::Off)],
            scope: Scope::Global,
        };
        let ro_arg_location = sv.variables[0].origin.clone();
        let w_location = sv.variables[1].origin.clone();

        let errors = sv.execute(&mut env).unwrap_err();

        assert_matches!(&errors[..], [ExecuteError::UndoReadOnlyVariable(error)] => {
            assert_eq!(error.name.value, "ro");
            assert_eq!(error.name.origin, ro_arg_location);
            assert_eq!(error.read_only_location, ro_location);
        });
        assert_eq!(env.variables.get("ro"), Some(&ro));
        // No error for variable `w`
        assert_eq!(
            env.variables.get("w"),
            Some(&Variable {
                value: Some(Value::scalar("foo")),
                last_assigned_location: Some(w_location),
                ..w
            })
        );
    }

    #[test]
    fn overwriting_readonly_variables() {
        let mut env = Env::new_virtual();
        let ro_location = Location::dummy("assign readonly");
        let mut ro = env.get_or_create_variable("ro", Scope::Global.into());
        ro.assign("readonly value", ro_location.clone()).unwrap();
        ro.make_read_only(ro_location.clone());
        let ro = ro.clone();

        let sv = SetVariables {
            variables: Field::dummies(["ro=foo"]),
            attrs: vec![],
            scope: Scope::Global,
        };
        let assigned_location = sv.variables[0].origin.clone();

        let errors = sv.execute(&mut env).unwrap_err();
        assert_matches!(&errors[..], [ExecuteError::AssignReadOnlyVariable(error)] => {
            assert_eq!(error.new_value, Value::scalar("foo"));
            assert_eq!(error.assigned_location, assigned_location);
            assert_eq!(error.read_only_location, ro_location);
        });
        assert_eq!(env.variables.get("ro"), Some(&ro));
    }

    #[test]
    fn hiding_readonly_variables() {
        let mut outer = Env::new_virtual();
        let assign_location = Location::dummy("assign");
        let mut var = outer.get_or_create_variable("var", Scope::Global.into());
        var.assign("VAR", assign_location.clone()).unwrap();
        var.make_read_only(assign_location.clone());
        let mut inner = outer.push_context(Context::default());
        let sv = SetVariables {
            variables: Field::dummies(["var=NEW"]),
            attrs: vec![(VariableAttr::ReadOnly, State::Off)],
            scope: Scope::Local,
        };
        let new_location = sv.variables[0].origin.clone();

        let result = sv.execute(&mut inner);

        assert_eq!(result, Ok("".to_string()));
        let var = inner.variables.get("var").unwrap();
        assert_eq!(var.value, Some(Value::scalar("NEW")));
        assert_eq!(var.last_assigned_location, Some(new_location));
        assert_eq!(var.read_only_location, None);
        Env::pop_context(inner);
        let var = outer.variables.get("var").unwrap();
        assert_eq!(var.value, Some(Value::scalar("VAR")));
        assert_eq!(var.last_assigned_location.as_ref(), Some(&assign_location));
        assert_eq!(var.read_only_location.as_ref(), Some(&assign_location));
    }

    #[test]
    fn combination_of_readonly_attributes() {
        let mut env = Env::new_virtual();
        let sv = SetVariables {
            variables: Field::dummies(["foo=FOO"]),
            attrs: vec![
                (VariableAttr::ReadOnly, State::On),
                (VariableAttr::ReadOnly, State::Off),
            ],
            scope: Scope::Local,
        };
        let foo_location = sv.variables[0].origin.clone();

        let errors = sv.execute(&mut env).unwrap_err();

        assert_matches!(&errors[..], [ExecuteError::UndoReadOnlyVariable(error)] => {
            assert_eq!(error.name.value, "foo");
            assert_eq!(error.name.origin, foo_location);
            assert_eq!(error.read_only_location, foo_location);
        });
        let var = env.variables.get("foo").unwrap();
        assert_eq!(var.value, Some(Value::scalar("FOO")));
        assert_eq!(var.last_assigned_location.as_ref(), Some(&foo_location));
        assert_eq!(var.read_only_location.as_ref(), Some(&foo_location));
    }
}
