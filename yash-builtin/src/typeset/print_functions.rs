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
use yash_env::function::Function;
use yash_env::function::FunctionSet;

impl PrintFunctions {
    /// Executes the command.
    pub fn execute(self, functions: &FunctionSet) -> Result<String, Vec<ExecuteError>> {
        let mut output = String::new();
        let mut errors = Vec::new();

        if self.functions.is_empty() {
            let mut functions = functions.iter().map(AsRef::as_ref).collect::<Vec<_>>();
            // TODO Honor the collation order in the locale.
            functions.sort_unstable_by_key(|function| &function.name);
            for function in functions {
                print_one(function, &self.attrs, &mut output);
            }
        } else {
            for name in self.functions {
                match functions.get(&name.value) {
                    Some(function) => print_one(function, &self.attrs, &mut output),
                    None => errors.push(ExecuteError::PrintUnsetFunction(name)),
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

fn print_one(function: &Function, filter_attrs: &[(FunctionAttr, State)], output: &mut String) {
    // Skip if the function does not match the filter.
    if filter_attrs
        .iter()
        .any(|&(attr, state)| attr.test(function) != state)
    {
        return;
    }

    // Do the formatting.
    let name = yash_quote::quoted(&function.name);
    if name.needs_quoting() {
        output.push_str("function ");
    }
    // TODO multiline pretty printing
    writeln!(output, "{}() {}", name, function.body).unwrap();

    if function.is_read_only() {
        writeln!(output, "typeset -fr {}", name).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::function::Function;
    use yash_env::option::State::{Off, On};
    use yash_syntax::syntax::FullCompoundCommand;

    #[test]
    fn printing_one_function() {
        let mut functions = FunctionSet::new();
        let foo = Function::new(
            "foo",
            "{ echo; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo location"),
        );
        let bar = Function::new(
            "bar",
            "{ ls; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("bar location"),
        );
        functions.define(foo).unwrap();
        functions.define(bar).unwrap();
        let pf = PrintFunctions {
            functions: Field::dummies(["foo"]),
            attrs: vec![],
        };

        let result = pf.execute(&functions).unwrap();
        assert_eq!(result, "foo() { echo; }\n");
    }

    #[test]
    fn printing_multiple_functions() {
        let mut functions = FunctionSet::new();
        for i in 1..=4 {
            functions
                .define(Function::new(
                    format!("foo{i}"),
                    format!("{{ echo {i}; }}")
                        .parse::<FullCompoundCommand>()
                        .unwrap(),
                    Location::dummy("foo location"),
                ))
                .unwrap();
        }
        let pf = PrintFunctions {
            functions: Field::dummies(["foo1", "foo2", "foo3"]),
            attrs: vec![],
        };

        assert_eq!(
            pf.execute(&functions).unwrap(),
            "foo1() { echo 1; }\nfoo2() { echo 2; }\nfoo3() { echo 3; }\n",
        );
    }

    #[test]
    fn printing_all_functions() {
        let mut functions = FunctionSet::new();
        for i in [2, 4, 3, 1] {
            functions
                .define(Function::new(
                    format!("foo{i}"),
                    format!("{{ echo {i}; }}")
                        .parse::<FullCompoundCommand>()
                        .unwrap(),
                    Location::dummy("foo location"),
                ))
                .unwrap();
        }
        let pf = PrintFunctions {
            functions: vec![],
            attrs: vec![],
        };

        // The result is sorted by function name.
        assert_eq!(
            pf.execute(&functions).unwrap(),
            "foo1() { echo 1; }\nfoo2() { echo 2; }\nfoo3() { echo 3; }\nfoo4() { echo 4; }\n",
        );
    }

    #[test]
    fn quoting_function_name() {
        let mut functions = FunctionSet::new();
        let function = Function::new(
            "a/b$",
            "{ echo; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("location"),
        );
        functions.define(function).unwrap();
        let pf = PrintFunctions {
            functions: Field::dummies(["a/b$"]),
            attrs: vec![],
        };

        let result = pf.execute(&functions).unwrap();
        assert_eq!(result, "function 'a/b$'() { echo; }\n");
    }

    #[test]
    fn printing_readonly_functions() {
        let mut functions = FunctionSet::new();
        let foo = Function::new(
            "foo",
            "{ echo; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("definition location"),
        )
        .make_read_only(Location::dummy("readonly location"));
        functions.define(foo).unwrap();
        let pf = PrintFunctions {
            functions: Field::dummies(["foo"]),
            attrs: vec![],
        };

        let result = pf.execute(&functions).unwrap();
        assert_eq!(result, "foo() { echo; }\ntypeset -fr foo\n");
    }

    #[test]
    fn selecting_readonly_functions() {
        let mut functions = FunctionSet::new();
        let foo = Function::new(
            "foo",
            "{ echo; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo location"),
        );
        let bar = Function::new(
            "bar",
            "{ ls; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("bar location"),
        )
        .make_read_only(Location::dummy("bar readonly location"));
        functions.define(foo).unwrap();
        functions.define(bar).unwrap();
        let pf = PrintFunctions {
            functions: vec![],
            attrs: vec![(FunctionAttr::ReadOnly, On)],
        };

        let result = pf.execute(&functions).unwrap();
        assert_eq!(result, "bar() { ls; }\ntypeset -fr bar\n");
    }

    #[test]
    fn selecting_non_readonly_functions() {
        let mut functions = FunctionSet::new();
        let foo = Function::new(
            "foo",
            "{ echo; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("foo location"),
        );
        let bar = Function::new(
            "bar",
            "{ ls; }".parse::<FullCompoundCommand>().unwrap(),
            Location::dummy("bar location"),
        )
        .make_read_only(Location::dummy("bar readonly location"));
        functions.define(foo).unwrap();
        functions.define(bar).unwrap();
        let pf = PrintFunctions {
            functions: vec![],
            attrs: vec![(FunctionAttr::ReadOnly, Off)],
        };

        let result = pf.execute(&functions).unwrap();
        assert_eq!(result, "foo() { echo; }\n");
    }

    #[test]
    fn function_not_found() {
        let foo = Field::dummy("foo");
        let bar = Field::dummy("bar");
        let pf = PrintFunctions {
            functions: vec![foo.clone(), bar.clone()],
            attrs: vec![],
        };

        assert_eq!(
            pf.execute(&FunctionSet::new()).unwrap_err(),
            [
                ExecuteError::PrintUnsetFunction(foo),
                ExecuteError::PrintUnsetFunction(bar),
            ],
        );
    }
}
