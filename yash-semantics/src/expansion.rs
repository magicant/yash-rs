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

//! Word expansion.
//!
//! The word expansion involves many kinds of operations described below.
//! The [`expand_words`] function carries out all of them and produces any
//! number of fields depending on the expanded word. The [`expand_word`]
//! function omits some of them to ensure that the result is a single field.
//!
//! # Initial expansion
//!
//! The [initial expansion](self::initial) is the first step of the word
//! expansion that evaluates a [`Word`] to a [`Phrase`](self::phrase). It is
//! performed by recursively calling [`Expand`] implementors' methods. Notable
//! (sub)expansions that may occur in the initial expansion include the tilde
//! expansion, parameter expansion, command substitution, and arithmetic
//! expansion.
//!
//! A successful initial expansion of a word usually results in a single-field
//! phrase. Still, it may yield any number of fields if the word contains a
//! parameter expansion of `$@` or `$*`.
//!
//! # Multi-field expansion
//!
//! The multi-field expansion is a group of operation steps performed after the
//! initial expansion to render final multi-field results.
//!
//! ## Brace expansion
//!
//! The brace expansion produces copies of a field containing a pair of braces.
//!
//! ## Field splitting
//!
//! The [field splitting](split) divides a field into smaller parts delimited by
//! a character contained in `$IFS`. Consequently, this operation removes empty
//! fields from the results of the previous steps.
//!
//! ## Pathname expansion
//!
//! The pathname expansion performs pattern matching on the name of existing
//! files to produce pathnames. This operation is also known as "globbing."
//!
//! # Quote removal and attribute stripping
//!
//! The [quote removal](self::quote_removal) drops characters quoting other
//! characters, and the [attribute stripping](self::attr_strip) converts
//! [`AttrField`]s into bare [`Field`]s. In [`expand_words`], the quote removal
//! is performed between the field splitting and pathname expansion, and the
//! attribute stripping is part of the pathname expansion. In [`expand_word`],
//! they are carried out as the last step of the whole expansion.

pub mod attr;
pub mod attr_strip;
pub mod initial;
pub mod phrase;
pub mod quote_removal;
pub mod split;

use self::attr::AttrChar;
use self::attr::AttrField;
use self::attr::Origin;
use self::initial::Expand;
use std::borrow::Cow;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_env::variable::ReadOnlyError;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;
use yash_syntax::source::Location;
use yash_syntax::syntax::Word;

#[doc(no_inline)]
pub use yash_env::semantics::Field;

/// Types of errors that may occur in the word expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    /// System error while performing a command substitution.
    CommandSubstError(Errno),
    /// Assignment to a read-only variable.
    AssignReadOnly(ReadOnlyError),
}

impl ErrorCause {
    /// Returns an error message describing the error.
    #[must_use]
    pub fn message(&self) -> &str {
        // TODO Localize
        use ErrorCause::*;
        match self {
            CommandSubstError(_) => "error performing the command substitution",
            AssignReadOnly(_) => "cannot assign to read-only variable",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> Cow<'_, str> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            CommandSubstError(e) => e.desc().into(),
            AssignReadOnly(e) => format!("variable `{}` is read-only", e.name).into(),
        }
    }

    /// Returns a location related with the error cause and a message describing
    /// the location.
    #[must_use]
    pub fn related_location(&self) -> Option<(&Location, &'static str)> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            CommandSubstError(_) => None,
            AssignReadOnly(e) => Some((
                &e.read_only_location,
                "the variable was made read-only here",
            )),
        }
    }
}

impl std::fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ErrorCause::*;
        match self {
            CommandSubstError(errno) => write!(f, "error in command substitution: {errno}"),
            AssignReadOnly(error) => error.fmt(f),
        }
    }
}

/// Explanation of an expansion failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl std::error::Error for Error {}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        self.cause.message().into()
    }

    fn main_annotation(&self) -> Annotation {
        Annotation::new(AnnotationType::Error, self.cause.label(), &self.location)
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        if let Some((location, label)) = self.cause.related_location() {
            // TODO Use Extend::extend_one
            results.extend(std::iter::once(Annotation::new(
                AnnotationType::Info,
                label.into(),
                location,
            )))
        }
    }
}

/// Result of word expansion.
pub type Result<T> = std::result::Result<T, Error>;

/// Expands a word to a field.
///
/// This function performs the initial expansion, quote removal, and attribute
/// stripping.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
///
/// To expand multiple words to multiple fields, use [`expand_words`].
pub async fn expand_word(
    env: &mut yash_env::Env,
    word: &Word,
) -> Result<(Field, Option<ExitStatus>)> {
    let mut env = initial::Env::new(env);
    // It would be technically correct to set `will_split` to false, but it does
    // not affect the final results because we will join the results anyway.
    // env.will_split = false;

    use self::initial::QuickExpand::*;
    let phrase = match word.quick_expand(&mut env) {
        Ready(result) => result?,
        Interim(interim) => word.async_expand(&mut env, interim).await?,
    };
    let chars = phrase.ifs_join(&env.inner.variables);
    let field = AttrField {
        chars,
        origin: word.location.clone(),
    };
    let field = field.remove_quotes_and_strip();
    Ok((field, env.last_command_subst_exit_status))
}

/// Expands words to fields.
///
/// This function performs the initial expansion and multi-field expansion,
/// including quote removal and attribute stripping.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
///
/// To expand a single word to a single field, use [`expand_word`].
pub async fn expand_words<'a, I: IntoIterator<Item = &'a Word>>(
    env: &mut yash_env::Env,
    words: I,
) -> Result<(Vec<Field>, Option<ExitStatus>)> {
    let words = words.into_iter();
    let mut fields = Vec::with_capacity(words.size_hint().0);
    let mut env = initial::Env::new(env);
    for word in words {
        use self::initial::QuickExpand::*;
        let phrase = match word.quick_expand(&mut env) {
            Ready(result) => result?,
            Interim(interim) => word.async_expand(&mut env, interim).await?,
        };
        fields.extend(phrase.into_iter().map(|chars| AttrField {
            chars,
            origin: word.location.clone(),
        }));
    }

    // TODO brace expansion
    // TODO field splitting
    // TODO pathname expansion (or quote removal and attribute stripping)

    let fields = fields
        .into_iter()
        .map(AttrField::remove_quotes_and_strip)
        .collect();
    Ok((fields, env.last_command_subst_exit_status))
}

/// Expands an assignment value.
///
/// This function expands a [`yash_syntax::syntax::Value`] to a
/// [`yash_env::variable::Value`]. A scalar and array value are expanded by
/// [`expand_word`] and [`expand_words`], respectively.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
pub async fn expand_value(
    env: &mut yash_env::Env,
    value: &yash_syntax::syntax::Value,
) -> Result<(yash_env::variable::Value, Option<ExitStatus>)> {
    match value {
        yash_syntax::syntax::Scalar(word) => {
            let (field, exit_status) = expand_word(env, word).await?;
            Ok((yash_env::variable::Scalar(field.value), exit_status))
        }
        yash_syntax::syntax::Array(words) => {
            let (fields, exit_status) = expand_words(env, words).await?;
            let fields = fields.into_iter().map(|f| f.value).collect();
            Ok((yash_env::variable::Array(fields), exit_status))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::num::NonZeroU64;
    use std::rc::Rc;
    use yash_env::variable::Value;
    use yash_env::variable::Variable;
    use yash_syntax::source::pretty::Message;
    use yash_syntax::source::Code;
    use yash_syntax::source::Source;

    #[test]
    fn from_error_for_message() {
        let code = Rc::new(Code {
            value: "hello".to_string().into(),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown,
        });
        let location = Location { code, range: 2..4 };
        let new_value = Variable {
            value: Value::Scalar("value".into()),
            last_assigned_location: Some(Location::dummy("assigned")),
            is_exported: false,
            read_only_location: None,
        };
        let error = Error {
            cause: ErrorCause::AssignReadOnly(ReadOnlyError {
                name: "var".into(),
                read_only_location: Location::dummy("ROL"),
                new_value,
            }),
            location,
        };
        let message = Message::from(&error);
        assert_eq!(message.r#type, AnnotationType::Error);
        assert_eq!(message.title, "cannot assign to read-only variable");
        assert_eq!(message.annotations.len(), 2);
        assert_eq!(message.annotations[0].r#type, AnnotationType::Error);
        assert_eq!(message.annotations[0].label, "variable `var` is read-only");
        assert_eq!(message.annotations[0].location, &error.location);
        assert_eq!(message.annotations[1].r#type, AnnotationType::Info);
        assert_eq!(
            message.annotations[1].label,
            "the variable was made read-only here"
        );
        assert_eq!(message.annotations[1].location, &Location::dummy("ROL"));
    }

    #[test]
    fn expand_value_scalar() {
        let mut env = yash_env::Env::new_virtual();
        let value = yash_syntax::syntax::Scalar(r"1\\".parse().unwrap());
        let (result, exit_status) = expand_value(&mut env, &value)
            .now_or_never()
            .unwrap()
            .unwrap();
        let content = assert_matches!(result, yash_env::variable::Scalar(content) => content);
        assert_eq!(content, r"1\");
        assert_eq!(exit_status, None);
    }

    #[test]
    fn expand_value_array() {
        let mut env = yash_env::Env::new_virtual();
        let value =
            yash_syntax::syntax::Array(vec!["''".parse().unwrap(), r"2\\".parse().unwrap()]);
        let (result, exit_status) = expand_value(&mut env, &value)
            .now_or_never()
            .unwrap()
            .unwrap();
        let content = assert_matches!(result, yash_env::variable::Array(content) => content);
        assert_eq!(content, ["".to_string(), r"2\".to_string()]);
        assert_eq!(exit_status, None);
    }
}
