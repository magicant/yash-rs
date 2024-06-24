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

//! Assigning the input to variables

use yash_env::semantics::Field;
use yash_env::variable::Scope;
use yash_env::variable::Value;
use yash_env::variable::Variable;
use yash_env::variable::IFS;
use yash_env::Env;
use yash_semantics::expansion::attr::AttrChar;
use yash_semantics::expansion::attr_strip::Strip as _;
use yash_semantics::expansion::quote_removal::skip_quotes;
use yash_semantics::expansion::split::Class;
use yash_semantics::expansion::split::Ifs;

pub use crate::typeset::AssignReadOnlyError as Error;

/// Assigns the text to variables.
///
/// This function performs field splitting on the text and assigns the resulting
/// fields to the variables. When there are more fields than variables, the last
/// variable receives all remaining fields, including the field separators, but
/// not trailing whitespace separators. When there are fewer fields than
/// variables, the remaining variables are set to empty strings.
///
/// The return value is a vector of errors that occurred while assigning the
/// variables. The vector is empty if no error occurred.
pub fn assign(
    env: &mut Env,
    text: &[AttrChar],
    variables: Vec<Field>,
    last_variable: Field,
) -> Vec<Error> {
    #[rustfmt::skip]
    let ifs = match env.variables.get(IFS) {
        Some(&Variable { value: Some(Value::Scalar(ref value)), ..  }) => value,
        // TODO If the variable is an array, should we ignore it?
        _ => Ifs::DEFAULT,
    };
    let ifs = ifs.to_owned();
    let ifs = Ifs::new(&ifs);

    let mut ranges = ifs.ranges(text.iter().copied());

    // Assign variables but the last
    let mut errors = variables
        .into_iter()
        .filter_map(|var_name| {
            let value = ranges.next().map(|r| &text[r]).unwrap_or_default();
            assign_one(env, var_name, value).err()
        })
        .collect::<Vec<_>>();

    // Assign the last
    let range = match ranges.next() {
        None => 0..0,
        Some(range) => match ranges.next() {
            None => range,
            Some(_range) => {
                let end = text
                    .iter()
                    .rposition(|&c| ifs.classify_attr(c) != Class::IfsWhitespace)
                    .unwrap()
                    + 1;
                range.start..end
            }
        },
    };
    let last_result = assign_one(env, last_variable, &text[range]);
    errors.extend(last_result.err());

    errors
}

/// Assigns one field to a variable.
fn assign_one(env: &mut Env, name: Field, value: &[AttrChar]) -> Result<(), Error> {
    let value = value.iter().copied();
    let value = skip_quotes(value).strip().collect::<String>();
    let mut var = env.get_or_create_variable(name.value.clone(), Scope::Global);
    match var.assign(value, name.origin) {
        Ok(_old_value) => Ok(()),
        Err(e) => Err(Error {
            name: name.value,
            new_value: e.new_value,
            assigned_location: e.assigned_location.unwrap(),
            read_only_location: e.read_only_location,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::variable::VariableSet;
    use yash_semantics::expansion::attr::Origin;
    use yash_syntax::source::Location;

    fn attr_chars(s: &str) -> Vec<AttrChar> {
        s.chars()
            .map(|c| AttrChar {
                value: c,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            })
            .collect()
    }

    fn assert_variable(vars: &VariableSet, name: &str, value: &str) {
        assert_matches!(
            vars.get(name),
            Some(Variable { value: Some(Value::Scalar(v)), ..  }) if v == value,
            "expected ${name}={value:?}",
        );
    }

    #[test]
    fn empty_text() {
        let mut env = Env::new_virtual();
        let origin = Location::dummy("var location");
        let var = Field {
            value: "var".into(),
            origin: origin.clone(),
        };

        let errors = assign(&mut env, &[], vec![], var);

        assert_eq!(errors, []);
        let var = env.variables.get("var").unwrap();
        assert_matches!(&var.value, Some(Value::Scalar(value)) if value.is_empty());
        assert_eq!(var.last_assigned_location, Some(origin));
    }

    #[test]
    fn single_variable_without_splitting() {
        let mut env = Env::new_virtual();
        let text = attr_chars("foo");

        let errors = assign(&mut env, &text, vec![], Field::dummy("var"));

        assert_eq!(errors, []);
        assert_variable(&env.variables, "var", "foo");
    }

    #[test]
    fn single_variable_with_trimming() {
        let mut env = Env::new_virtual();
        let text = attr_chars(" bar ");

        let errors = assign(&mut env, &text, vec![], Field::dummy("var"));

        assert_eq!(errors, []);
        assert_variable(&env.variables, "var", "bar");
    }

    #[test]
    fn many_variables() {
        let mut env = Env::new_virtual();
        let text = attr_chars(" 1 222  33 ");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "1");
        assert_variable(&env.variables, "second", "222");
        assert_variable(&env.variables, "last", "33");
    }

    #[test]
    fn more_variables_than_fields() {
        let mut env = Env::new_virtual();
        let text = attr_chars("foo");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "foo");
        assert_variable(&env.variables, "second", "");
        assert_variable(&env.variables, "last", "");
    }

    #[test]
    fn less_variables_than_fields() {
        let mut env = Env::new_virtual();
        let text = attr_chars(" 1 222 33  4 ");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "1");
        assert_variable(&env.variables, "last", "222 33  4");
    }

    #[test]
    fn non_default_ifs() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable(IFS, Scope::Global)
            .assign(" *", None)
            .unwrap();
        let text = attr_chars("1\t22 * * 333");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "1\t22");
        assert_variable(&env.variables, "second", "");
        assert_variable(&env.variables, "last", "333");
    }

    #[test]
    fn non_default_ifs_with_empty_field() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable(IFS, Scope::Global)
            .assign(" *", None)
            .unwrap();
        let text = attr_chars("1 22 * * 333 ");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "1");
        assert_variable(&env.variables, "second", "22");
        assert_variable(&env.variables, "last", "* 333");
    }

    #[test]
    fn non_default_ifs_delimiting_last_field() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable(IFS, Scope::Global)
            .assign(" *", None)
            .unwrap();
        let text = attr_chars("1 22 * 333 * ");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "1");
        assert_variable(&env.variables, "second", "22");
        // The input text contains exactly three fields, so the last variable
        // does not receive the trailing field separator.
        assert_variable(&env.variables, "last", "333");
    }

    #[test]
    fn non_default_ifs_delimiting_last_field_extra() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable(IFS, Scope::Global)
            .assign(" *", None)
            .unwrap();
        let text = attr_chars("1 22 * 333 44  * ");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", "1");
        assert_variable(&env.variables, "second", "22");
        // The input text contains more than three fields, so the last variable
        // receives the trailing field separator.
        assert_variable(&env.variables, "last", "333 44  *");
    }

    #[test]
    fn quote_removal() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable(IFS, Scope::Global)
            .assign(r" \", None)
            .unwrap();
        let text = vec![
            AttrChar {
                value: '1',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            },
            AttrChar {
                value: '\\',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: true,
            },
            AttrChar {
                value: ' ',
                origin: Origin::SoftExpansion,
                is_quoted: true,
                is_quoting: false,
            },
            AttrChar {
                value: '\\',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: true,
            },
            AttrChar {
                value: '\\',
                origin: Origin::SoftExpansion,
                is_quoted: true,
                is_quoting: false,
            },
            AttrChar {
                value: '\\',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: true,
            },
            AttrChar {
                value: '2',
                origin: Origin::SoftExpansion,
                is_quoted: true,
                is_quoting: false,
            },
        ];

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first"]),
            Field::dummy("last"),
        );

        assert_eq!(errors, []);
        assert_variable(&env.variables, "first", r"1 \2");
        assert_variable(&env.variables, "last", "");
    }

    #[test]
    fn read_only_variables() {
        let mut env = Env::new_virtual();
        env.get_or_create_variable("first", Scope::Global)
            .make_read_only(Location::dummy("first read-only"));
        env.get_or_create_variable("last", Scope::Global)
            .make_read_only(Location::dummy("last read-only"));
        let text = attr_chars(" 1 222  33 ");

        let errors = assign(
            &mut env,
            &text,
            Field::dummies(["first", "second"]),
            Field::dummy("last"),
        );

        assert_matches!(&errors[..], [first, last] => {
            assert_eq!(first, &Error {
                name: "first".into(),
                new_value: "1".into(),
                assigned_location: Location::dummy("first"),
                read_only_location: Location::dummy("first read-only"),
            });
            assert_eq!(last, &Error {
                name: "last".into(),
                new_value: "33".into(),
                assigned_location: Location::dummy("last"),
                read_only_location: Location::dummy("last read-only"),
            });
        });
        assert_variable(&env.variables, "second", "222");
    }
}
