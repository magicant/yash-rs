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
use std::fmt::Write;
use yash_env::variable::{Value, VariableSet};

impl PrintVariables {
    /// Executes the command.
    pub fn execute(
        self,
        variables: &VariableSet,
        context: &PrintVariablesContext,
    ) -> Result<String, Vec<ExecuteError>> {
        let mut output = String::new();
        let mut errors = Vec::new();

        if self.variables.is_empty() {
            let mut variables = variables.iter(self.scope.into()).collect::<Vec<_>>();
            // TODO Honor the collation order in the locale.
            variables.sort_unstable_by_key(|&(name, _)| name);
            for (name, var) in variables {
                print_one(name, var, &self.attrs, context, &mut output);
            }
        } else {
            for name in self.variables {
                match variables.get_scoped(&name.value, self.scope.into()) {
                    Some(var) => print_one(&name.value, var, &self.attrs, context, &mut output),
                    None => errors.push(ExecuteError::PrintUnsetVariable(name)),
                }
            }
        }

        if errors.is_empty() {
            Ok(output)
        } else {
            Err(errors)
        }
    }
}

/// Formats a variable for printing.
fn print_one(
    name: &str,
    var: &Variable,
    filter_attrs: &[(VariableAttr, State)],
    context: &PrintVariablesContext,
    output: &mut String,
) {
    // Skip if the variable does not match the filter.
    if filter_attrs
        .iter()
        .any(|&(attr, state)| attr.test(var) != state)
    {
        return;
    }

    // Do the formatting.
    let options = AttributeOption { var };
    let quoted_name = yash_quote::quoted(name);
    match &var.value {
        Some(value @ Value::Scalar(_)) => writeln!(
            output,
            "{} {}{}={}",
            context.builtin_name,
            options,
            quoted_name,
            value.quote()
        )
        .unwrap(),

        Some(value @ Value::Array(_)) => {
            writeln!(output, "{}={}", quoted_name, value.quote()).unwrap();

            let options = options.to_string();
            if !options.is_empty() {
                writeln!(
                    output,
                    "{} {}{}",
                    context.builtin_name, options, quoted_name
                )
                .unwrap();
            }
        }

        None => writeln!(
            output,
            "{} {}{}",
            context.builtin_name, options, quoted_name
        )
        .unwrap(),
    }
}

/// `Display` implementor for printing command line options that reproduce the
/// variable attributes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AttributeOption<'a> {
    var: &'a Variable,
}

impl std::fmt::Display for AttributeOption<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO Filter printable options
        if self.var.is_read_only() {
            f.write_str("-r ")?;
        }
        if self.var.is_exported {
            f.write_str("-x ")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::option::{Off, On};
    use yash_env::variable::Context;

    #[test]
    fn printing_one_variable() {
        let mut vars = VariableSet::new();
        vars.get_or_new("foo", Scope::Global.into())
            .assign("value", None)
            .unwrap();
        let pv = PrintVariables {
            variables: Field::dummies(["foo"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        let output = pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap();
        assert_eq!(output, "typeset foo=value\n")
    }

    #[test]
    fn printing_multiple_variables() {
        let mut vars = VariableSet::new();
        vars.get_or_new("first", Scope::Global.into())
            .assign("1", None)
            .unwrap();
        vars.get_or_new("second", Scope::Global.into())
            .assign("2", None)
            .unwrap();
        vars.get_or_new("third", Scope::Global.into())
            .assign("3", None)
            .unwrap();
        let pv = PrintVariables {
            variables: Field::dummies(["first", "second", "third"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset first=1\n\
             typeset second=2\n\
             typeset third=3\n",
        );
    }

    #[test]
    fn printing_array_variable() {
        let mut vars = VariableSet::new();
        vars.get_or_new("a", Scope::Global.into())
            .assign(Value::array(["1", "2  2", "3"]), None)
            .unwrap();
        let pv = PrintVariables {
            variables: Field::dummies(["a"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        let result = pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap();
        assert_eq!(result, "a=(1 '2  2' 3)\n");
    }

    #[test]
    fn printing_valueless_variable() {
        let mut vars = VariableSet::new();
        vars.get_or_new("x", Scope::Global.into());
        let pv = PrintVariables {
            variables: Field::dummies(["x"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        let result = pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap();
        assert_eq!(result, "typeset x\n");
    }

    #[test]
    fn quoting_variable_names_and_values() {
        let mut vars = VariableSet::new();
        vars.get_or_new("valueless$", Scope::Global.into());
        vars.get_or_new("scalar$", Scope::Global.into())
            .assign("=;", None)
            .unwrap();
        vars.get_or_new("array$", Scope::Global.into())
            .assign(Value::array(["~", "'", "*?"]), None)
            .unwrap();
        let pv = PrintVariables {
            variables: Field::dummies(["valueless$", "scalar$", "array$"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset 'valueless$'\n\
             typeset 'scalar$'='=;'\n\
             'array$'=('~' \"'\" '*?')\n",
        );
    }

    #[test]
    fn printing_global_and_local_variables_at_once() {
        let mut outer = VariableSet::new();
        outer
            .get_or_new("global", Scope::Global.into())
            .assign("global value", None)
            .unwrap();
        let mut inner = outer.push_context(Context::default());
        inner
            .get_or_new("local", Scope::Local.into())
            .assign("local value", None)
            .unwrap();
        let pv = PrintVariables {
            variables: Field::dummies(["global", "local"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&inner, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset global='global value'\n\
             typeset local='local value'\n",
        );
    }

    #[test]
    fn printing_local_variables_only() {
        let mut outer = VariableSet::new();
        outer
            .get_or_new("global", Scope::Global.into())
            .assign("global value", None)
            .unwrap();
        let mut inner = outer.push_context(Context::default());
        inner
            .get_or_new("local", Scope::Local.into())
            .assign("local value", None)
            .unwrap();

        let pv = PrintVariables {
            variables: Field::dummies(["local"]),
            attrs: vec![],
            scope: Scope::Local,
        };
        let output = pv.execute(&inner, &PRINT_VARIABLES_CONTEXT).unwrap();
        assert_eq!(output, "typeset local='local value'\n");

        let pv = PrintVariables {
            variables: Field::dummies(["global"]),
            attrs: vec![],
            scope: Scope::Local,
        };
        assert_eq!(
            pv.execute(&inner, &PRINT_VARIABLES_CONTEXT).unwrap_err(),
            [ExecuteError::PrintUnsetVariable(Field::dummy("global"))]
        );
    }

    #[test]
    fn printing_all_global_and_local_variables() {
        let mut outer = VariableSet::new();
        outer
            .get_or_new("one", Scope::Global.into())
            .assign("1", None)
            .unwrap();
        let mut inner = outer.push_context(Context::default());
        inner
            .get_or_new("two", Scope::Local.into())
            .assign("2", None)
            .unwrap();
        inner
            .get_or_new("three", Scope::Local.into())
            .assign("3", None)
            .unwrap();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&inner, &PRINT_VARIABLES_CONTEXT).unwrap(),
            // sorted by name
            "typeset one=1\n\
             typeset three=3\n\
             typeset two=2\n",
        );
    }

    #[test]
    fn printing_all_local_variables() {
        let mut outer = VariableSet::new();
        outer
            .get_or_new("one", Scope::Global.into())
            .assign("1", None)
            .unwrap();
        let mut inner = outer.push_context(Context::default());
        inner
            .get_or_new("two", Scope::Local.into())
            .assign("2", None)
            .unwrap();
        inner
            .get_or_new("three", Scope::Local.into())
            .assign("3", None)
            .unwrap();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![],
            scope: Scope::Local,
        };

        assert_eq!(
            pv.execute(&inner, &PRINT_VARIABLES_CONTEXT).unwrap(),
            // sorted by name
            "typeset three=3\n\
             typeset two=2\n",
        );
    }

    #[test]
    fn printing_attributes_of_valueless_variables() {
        let mut vars = VariableSet::new();
        let mut x = vars.get_or_new("x", Scope::Global.into());
        x.export(true);
        let mut y = vars.get_or_new("y", Scope::Global.into());
        y.make_read_only(Location::dummy("y location"));
        let mut z = vars.get_or_new("z", Scope::Global.into());
        z.export(true);
        z.make_read_only(Location::dummy("z location"));
        let pv = PrintVariables {
            variables: Field::dummies(["x", "y", "z"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset -x x\n\
             typeset -r y\n\
             typeset -r -x z\n",
        );
    }

    #[test]
    fn printing_attributes_of_scalar_variables() {
        let mut vars = VariableSet::new();
        let mut x = vars.get_or_new("x", Scope::Global.into());
        x.assign("X", None).unwrap();
        x.export(true);
        let mut y = vars.get_or_new("y", Scope::Global.into());
        y.assign("Y", None).unwrap();
        y.make_read_only(Location::dummy("y location"));
        let mut z = vars.get_or_new("z", Scope::Global.into());
        z.assign("Z", None).unwrap();
        z.export(true);
        z.make_read_only(Location::dummy("z location"));
        let pv = PrintVariables {
            variables: Field::dummies(["x", "y", "z"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset -x x=X\n\
             typeset -r y=Y\n\
             typeset -r -x z=Z\n",
        );
    }

    #[test]
    fn printing_attributes_of_array_variables() {
        let mut vars = VariableSet::new();
        let mut x = vars.get_or_new("x", Scope::Global.into());
        x.assign(Value::array(["X"]), None).unwrap();
        x.export(true);
        let mut y = vars.get_or_new("y", Scope::Global.into());
        y.assign(Value::array(["Y"]), None).unwrap();
        y.make_read_only(Location::dummy("y location"));
        let mut z = vars.get_or_new("z", Scope::Global.into());
        z.assign(Value::array(["Z"]), None).unwrap();
        z.export(true);
        z.make_read_only(Location::dummy("z location"));
        let pv = PrintVariables {
            variables: Field::dummies(["x", "y", "z"]),
            attrs: vec![],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "x=(X)\n\
             typeset -x x\n\
             y=(Y)\n\
             typeset -r y\n\
             z=(Z)\n\
             typeset -r -x z\n",
        );
    }

    fn variables_with_different_attributes() -> VariableSet {
        let mut vars = VariableSet::new();
        let mut a = vars.get_or_new("a", Scope::Global.into());
        a.export(true);
        let mut b = vars.get_or_new("b", Scope::Global.into());
        b.make_read_only(Location::dummy("b location"));
        let mut c = vars.get_or_new("c", Scope::Global.into());
        c.export(true);
        c.make_read_only(Location::dummy("c location"));
        vars.get_or_new("d", Scope::Global.into());
        vars
    }

    #[test]
    fn selecting_readonly_variables() {
        let vars = variables_with_different_attributes();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![(VariableAttr::ReadOnly, On)],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset -r b\n\
             typeset -r -x c\n",
        );
    }

    #[test]
    fn selecting_non_readonly_variables() {
        let vars = variables_with_different_attributes();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![(VariableAttr::ReadOnly, Off)],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset -x a\n\
             typeset d\n",
        );
    }

    #[test]
    fn selecting_exported_variables() {
        let vars = variables_with_different_attributes();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![(VariableAttr::Export, On)],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset -x a\n\
             typeset -r -x c\n",
        );
    }

    #[test]
    fn selecting_non_exported_variables() {
        let vars = variables_with_different_attributes();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![(VariableAttr::Export, Off)],
            scope: Scope::Global,
        };

        assert_eq!(
            pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap(),
            "typeset -r b\n\
             typeset d\n",
        );
    }

    #[test]
    fn selecting_with_multiple_filtering_attributes() {
        let vars = variables_with_different_attributes();
        let pv = PrintVariables {
            variables: vec![],
            attrs: vec![(VariableAttr::ReadOnly, On), (VariableAttr::Export, Off)],
            scope: Scope::Global,
        };

        let result = pv.execute(&vars, &PRINT_VARIABLES_CONTEXT).unwrap();
        assert_eq!(result, "typeset -r b\n");
    }

    #[test]
    fn variable_not_found() {
        let foo = Field::dummy("foo");
        let bar = Field::dummy("bar");
        let pv = PrintVariables {
            variables: vec![foo.clone(), bar.clone()],
            attrs: vec![],
            scope: Scope::Global,
        };

        let error = pv
            .execute(&VariableSet::new(), &PRINT_VARIABLES_CONTEXT)
            .unwrap_err();
        assert_eq!(
            error,
            [
                ExecuteError::PrintUnsetVariable(foo),
                ExecuteError::PrintUnsetVariable(bar)
            ]
        );
    }

    mod non_default_context {
        use super::*;

        #[test]
        fn builtin_name() {
            let mut vars = VariableSet::new();
            vars.get_or_new("foo", Scope::Global.into())
                .assign("value", None)
                .unwrap();
            let bar = &mut vars.get_or_new("bar", Scope::Global.into());
            bar.assign(Value::array(["1", "2"]), None).unwrap();
            bar.make_read_only(Location::dummy("bar location"));
            vars.get_or_new("baz", Scope::Global.into());
            let pv = PrintVariables {
                variables: Field::dummies(["foo", "bar", "baz"]),
                attrs: vec![],
                scope: Scope::Global,
            };
            let context = PrintVariablesContext {
                builtin_name: "export",
            };

            assert_eq!(
                pv.execute(&vars, &context).unwrap(),
                "export foo=value\n\
                 bar=(1 2)\n\
                 export -r bar\n\
                 export baz\n"
            );
        }
    }
}
