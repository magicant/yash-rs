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

//! Core runtime behavior of the alias built-in

use super::Command;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_quote::quoted;
use yash_syntax::alias::Alias;
use yash_syntax::alias::HashEntry;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

/// Error in executing the alias built-in
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// Printing a non-existent alias
    #[error("alias {name} not found")]
    NonExistentAlias { name: Field },
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<'_, str> {
        "cannot print alias definition".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        match self {
            Error::NonExistentAlias { name } => {
                Annotation::new(AnnotationType::Error, self.to_string().into(), &name.origin)
            }
        }
    }
}

/// Defines an alias.
///
/// If `name_value` is of the form `name=value`, defines an alias named `name`
/// that expands to `value`. Otherwise, returns `Err(name_value)`.
fn define(env: &mut Env, name_value: Field) -> Result<(), Field> {
    let Some(equal) = name_value.value.find('=') else {
        return Err(name_value);
    };
    let replacement = name_value.value[equal + 1..].to_owned();
    let name = {
        let mut name = name_value.value;
        name.truncate(equal);
        // TODO Reject invalid name
        name.shrink_to_fit();
        name
    };
    // TODO Support global aliases
    let global = false;

    env.aliases
        .replace(HashEntry::new(name, replacement, global, name_value.origin));

    Ok(())
}

/// Prints the definition of an alias.
///
/// This function appends a string of the form `name=value\n` to `result`.
/// If the named alias does not exist, returns an error.
fn find_and_print(env: &Env, name: Field, result: &mut String) -> Result<(), Error> {
    let alias = env
        .aliases
        .get(name.value.as_str())
        .ok_or(Error::NonExistentAlias { name })?;

    print(&alias.0, result);

    Ok(())
}

/// Prints the definition of an alias.
/// This function appends a string of the form `name=value\n` to `result`.
fn print(alias: &Alias, result: &mut String) {
    use std::fmt::Write as _;
    writeln!(
        result,
        "{}={}",
        quoted(&alias.name),
        quoted(&alias.replacement),
    )
    .unwrap();
}

impl Command {
    /// Executes the alias built-in
    ///
    /// Returns a string that contains the alias definitions to be printed and a
    /// list of errors that occurred during the execution.
    pub async fn execute(self, env: &mut Env) -> (String, Vec<Error>) {
        let mut output = String::new();
        let mut errors = Vec::new();

        if self.operands.is_empty() {
            // Make a temporary vector to sort the aliases by name
            let mut aliases = env.aliases.iter().collect::<Vec<_>>();
            // TODO Locale-aware sorting
            aliases.sort_unstable_by_key(|alias| &alias.0.name);
            for alias in aliases {
                print(&alias.0, &mut output);
            }
        } else {
            for operand in self.operands {
                if let Err(operand) = define(env, operand) {
                    let result = find_and_print(env, operand, &mut output);
                    errors.extend(result.err());
                }
            }
        }

        (output, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use yash_syntax::alias::Alias;
    use yash_syntax::source::Location;

    #[test]
    fn defining_alias() {
        let mut env = Env::new_virtual();
        let origin = Location::dummy("definition location");

        let result = define(
            &mut env,
            Field {
                value: "foo=bar".into(),
                origin: origin.clone(),
            },
        );

        assert_eq!(result, Ok(()));
        assert_eq!(
            *env.aliases.get("foo").unwrap().0,
            Alias {
                name: "foo".into(),
                replacement: "bar".into(),
                global: false,
                origin,
            }
        );
    }

    #[test]
    fn defining_alias_without_value() {
        let mut env = Env::new_virtual();
        let field = Field::dummy("valueless");
        let result = define(&mut env, field.clone());
        assert_eq!(result, Err(field));
        assert_eq!(env.aliases.len(), 0);
    }

    #[test]
    fn finding_and_printing_alias() {
        let mut env = Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "foo".into(),
            "bar".into(),
            false,
            Location::dummy("definition location"),
        ));
        let mut result = String::new();

        let return_value = find_and_print(&env, Field::dummy("foo"), &mut result);

        assert_eq!(return_value, Ok(()));
        assert_eq!(result, "foo=bar\n");
    }

    #[test]
    fn finding_non_existent_alias() {
        let name = Field::dummy("foo");
        let mut result = String::new();

        let return_value = find_and_print(&Env::new_virtual(), name.clone(), &mut result);

        assert_eq!(return_value, Err(Error::NonExistentAlias { name }));
        assert_eq!(result, "");
    }

    #[test]
    fn printing_quoted_alias_name() {
        let alias = Alias {
            name: "foo bar".into(),
            replacement: "x".into(),
            global: false,
            origin: Location::dummy("definition location"),
        };
        let mut result = String::new();

        print(&alias, &mut result);

        assert_eq!(result, "'foo bar'=x\n");
    }

    #[test]
    fn printing_quoted_alias_value() {
        let alias = Alias {
            name: "ll".into(),
            replacement: "ls -l".into(),
            global: false,
            origin: Location::dummy("definition location"),
        };
        let mut result = String::new();

        print(&alias, &mut result);

        assert_eq!(result, "ll='ls -l'\n");
    }

    #[test]
    fn executing_with_operands() {
        let mut env = Env::new_virtual();
        let operands = Field::dummies(["foo=bar", "bar", "foo"]);
        let command = Command { operands };

        let (output, errors) = command.execute(&mut env).now_or_never().unwrap();

        assert_eq!(output, "foo=bar\n");
        assert_eq!(
            errors,
            [Error::NonExistentAlias {
                name: Field::dummy("bar")
            }]
        );
    }

    #[test]
    fn executing_with_no_operands() {
        let mut env = Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "foo".into(),
            "bar".into(),
            false,
            Location::dummy("foo location"),
        ));
        env.aliases.insert(HashEntry::new(
            "ll".into(),
            "ls -l".into(),
            false,
            Location::dummy("ll location"),
        ));
        env.aliases.insert(HashEntry::new(
            "ls".into(),
            "ls --color".into(),
            false,
            Location::dummy("ls location"),
        ));
        env.aliases.insert(HashEntry::new(
            "cat".into(),
            "cat".into(),
            false,
            Location::dummy("cat location"),
        ));

        let command = Command { operands: vec![] };

        let (output, errors) = command.execute(&mut env).now_or_never().unwrap();
        // The output is sorted by name
        assert_eq!(output, "cat=cat\nfoo=bar\nll='ls -l'\nls='ls --color'\n");
        assert_eq!(errors, []);
    }
}
