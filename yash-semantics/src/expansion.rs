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
//! The word expansion involves many kinds of operations grouped into the
//! categories described below. The [`expand_words`] function carries out all of
//! them.
//!
//! # Initial expansion
//!
//! The initial expansion converts a word fragment to attributed characters
//! ([`AttrChar`]). It may involve the tilde expansion, parameter expansion,
//! command substitution, and arithmetic expansion performed by the [`Expand`]
//! implementors.
//!
//! Depending on the context, you can configure the expansion to produce either
//! a single field or any number of fields. Using `Vec<AttrChar>` as
//! [`Expansion`] will result in a single field. `Vec<Vec<AttrChar>>` may yield
//! any number of fields.
//!
//! To perform the initial expansion on a text/word fragment that implements
//! `Expand`, you call [`expand`](Expand::expand) on the text/word instance by
//! providing an [`Env`] and [`Output`]. You can create the `Output` from an
//! [`Expansion`] implementor. If successful, the `Expansion` implementor will
//! contain the result.
//!
//! To expand a whole [word](Word), you can instead call a method of
//! [`ExpandToField`]. It produces [`AttrField`]s instead of `AttrChar` vectors.
//!
//! # Multi-field expansion
//!
//! In a context expecting any number of fields, the results of the initial
//! expansion can be subjected to the multi-field expansion. It consists of the
//! brace expansion, field splitting, and pathname expansion, performed in this
//! order. The field splitting includes empty field removal, and the pathname
//! expansion includes the quote removal described below.
//!
//! (TBD: How do users perform multi-field expansion?)
//!
//! # Quote removal
//!
//! The [quote removal](QuoteRemoval) is the last step of the word expansion
//! that removes quotes from the field. It takes an [`AttrField`] input and
//! returns a [`Field`].

pub mod attr;
pub mod initial;
pub mod phrase;
pub mod quote_removal;

use self::attr::AttrChar;
use self::attr::AttrField;
use self::attr::Origin;
use self::quote_removal::*;
use std::borrow::Cow;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_env::variable::ReadOnlyError;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;
use yash_syntax::syntax::Word;

#[doc(no_inline)]
pub use yash_env::semantics::Field;

/// Types of errors that may occur in the word expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    // TODO Define error cause types
    Dummy(String),
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
            Dummy(message) => message,
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
            Dummy(_) => "".into(),
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
            Dummy(_) | CommandSubstError(_) => None,
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
            Dummy(message) => message.fmt(f),
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

impl<'a> From<&'a Error> for Message<'a> {
    fn from(e: &'a Error) -> Self {
        let mut a = vec![Annotation::new(
            AnnotationType::Error,
            e.cause.label(),
            &e.location,
        )];

        e.location.code.source.complement_annotations(&mut a);

        if let Some((location, label)) = e.cause.related_location() {
            a.push(Annotation::new(
                AnnotationType::Info,
                label.into(),
                location,
            ));
        }

        Message {
            r#type: AnnotationType::Error,
            title: e.cause.message().into(),
            annotations: a,
        }
    }
}

/// Result of word expansion.
pub type Result<T> = std::result::Result<T, Error>;

/// Expands a word to a field.
///
/// This function performs the initial expansion and quote removal.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
///
/// To expand multiple words to multiple fields, use [`expand_words`].
pub async fn expand_word(
    env: &mut yash_env::Env,
    word: &Word,
) -> Result<(Field, Option<ExitStatus>)> {
    use self::initial::Expand;
    use self::initial::QuickExpand::*;
    let mut env = initial::Env::new(env);
    let phrase = match word.quick_expand(&mut env) {
        Ready(result) => result?,
        Interim(interim) => word.async_expand(&mut env, interim).await?,
    };
    let chars = phrase.ifs_join(&env.inner.variables);
    let field = AttrField {
        chars,
        origin: word.location.clone(),
    };
    let field = field.do_quote_removal();
    Ok((field, env.last_command_subst_exit_status))
}

/// Expands words to fields.
///
/// This function performs the initial expansion and multi-field expansion,
/// including quote removal.
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
        use self::initial::Expand;
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
        .map(QuoteRemoval::do_quote_removal)
        .collect();
    Ok((fields, env.last_command_subst_exit_status))
}

/// Expands an assignment value.
///
/// This function expands a [`yash_syntax::syntax::Value`] to a
/// [`yash_env::variable::Value`]. A scalar and array value are expanded by
/// [`expand_word`] and [`expand_words`], respectively.
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
