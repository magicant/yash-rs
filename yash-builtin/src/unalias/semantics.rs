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

//! Core runtime behavior of the `unalias` built-in

use super::Command;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

/// Errors that can occur while executing the `unalias` built-in
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// The operand names a non-existent alias.
    #[error("no such alias `{0}`")]
    UndefinedAlias(Field),
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        "cannot remove alias".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        match self {
            Error::UndefinedAlias(alias) => Annotation::new(
                AnnotationType::Error,
                format!("no such alias `{alias}`").into(),
                &alias.origin,
            ),
        }
    }
}

impl Command {
    /// Executes the `unalias` built-in.
    ///
    /// Returns a list of errors that occurred while executing the built-in.
    #[must_use]
    pub fn execute(self, env: &mut Env) -> Vec<Error> {
        match self {
            Command::RemoveAll => {
                env.aliases.clear();
                vec![]
            }
            Command::Remove(operands) => operands
                .into_iter()
                .filter(|operand| !env.aliases.remove(operand.value.as_str()))
                .map(Error::UndefinedAlias)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_syntax::alias::HashEntry;
    use yash_syntax::source::Location;

    #[test]
    fn remove_all() {
        let mut env = Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "foo".into(),
            "FOO".into(),
            false,
            Location::dummy("foo location"),
        ));
        env.aliases.insert(HashEntry::new(
            "bar".into(),
            "BAR".into(),
            false,
            Location::dummy("bar location"),
        ));

        let errors = Command::RemoveAll.execute(&mut env);

        assert_eq!(errors, []);
        assert_eq!(env.aliases.len(), 0, "remaining: {:?}", env.aliases);
    }

    #[test]
    fn remove_some() {
        let mut env = Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "foo".into(),
            "FOO".into(),
            false,
            Location::dummy("foo location"),
        ));
        let bar = HashEntry::new(
            "bar".into(),
            "BAR".into(),
            false,
            Location::dummy("bar location"),
        );
        env.aliases.insert(bar.clone());
        env.aliases.insert(HashEntry::new(
            "baz".into(),
            "BAZ".into(),
            false,
            Location::dummy("baz location"),
        ));
        let names = Field::dummies(["foo", "baz"]);

        let errors = Command::Remove(names).execute(&mut env);

        assert_eq!(errors, []);
        let aliases = env.aliases.into_iter().collect::<Vec<_>>();
        assert_eq!(aliases, [bar]);
    }

    #[test]
    fn remove_undefined() {
        let mut env = Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "foo".into(),
            "FOO".into(),
            false,
            Location::dummy("foo location"),
        ));
        let bar = HashEntry::new(
            "bar".into(),
            "BAR".into(),
            false,
            Location::dummy("bar location"),
        );
        env.aliases.insert(bar.clone());
        env.aliases.insert(HashEntry::new(
            "baz".into(),
            "BAZ".into(),
            false,
            Location::dummy("baz location"),
        ));
        let names = Field::dummies(["foo", "gar", "baz", "qux"]);

        let errors = Command::Remove(names).execute(&mut env);

        assert_eq!(
            errors,
            [
                Error::UndefinedAlias(Field::dummy("gar")),
                Error::UndefinedAlias(Field::dummy("qux")),
            ]
        );
        // Despite the errors, the existing aliases are removed.
        let aliases = env.aliases.into_iter().collect::<Vec<_>>();
        assert_eq!(aliases, [bar]);
    }
}
