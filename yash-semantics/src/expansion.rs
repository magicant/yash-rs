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
//! number of fields depending on the expanded word. The [`expand_word_attr`]
//! and [`expand_word`] functions omit some of them to ensure that the result is
//! a single field.
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
//! The [pathname expansion](mod@glob) performs pattern matching on the name of
//! existing files to produce pathnames. This operation is also known as
//! "globbing."
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
pub mod glob;
pub mod initial;
pub mod phrase;
pub mod quote_removal;
pub mod split;

use self::attr::AttrChar;
use self::attr::AttrField;
use self::attr::Origin;
use self::attr_strip::Strip;
use self::glob::glob;
use self::initial::expand;
use self::initial::ArithError;
use self::initial::EmptyError;
#[cfg(doc)]
use self::initial::Expand;
use self::initial::NonassignableError;
use self::quote_removal::skip_quotes;
use self::split::Ifs;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_env::variable::ReadOnlyError;
use yash_env::variable::Variable;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;
use yash_syntax::source::Location;
use yash_syntax::syntax::Text;
use yash_syntax::syntax::Word;

#[doc(no_inline)]
pub use yash_env::semantics::Field;

/// Types of errors that may occur in the word expansion.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ErrorCause {
    /// System error while performing a command substitution.
    #[error("error in command substitution: {0}")]
    CommandSubstError(Errno),

    /// Error while evaluating an arithmetic expansion.
    #[error(transparent)]
    ArithError(ArithError),

    /// Assignment to a read-only variable.
    #[error(transparent)]
    AssignReadOnly(ReadOnlyError),

    /// Expansion of an unset parameter with the `nounset` option
    #[error("unset parameter")]
    UnsetParameter,

    /// Expansion of an empty value with an error switch
    #[error(transparent)]
    EmptyExpansion(EmptyError),

    /// Assignment to a nonassignable parameter
    #[error(transparent)]
    NonassignableParameter(NonassignableError),
}

impl ErrorCause {
    /// Returns an error message describing the error.
    #[must_use]
    pub fn message(&self) -> &str {
        // TODO Localize
        use ErrorCause::*;
        match self {
            CommandSubstError(_) => "error performing the command substitution",
            ArithError(_) => "error evaluating the arithmetic expansion",
            AssignReadOnly(_) => "cannot assign to read-only variable",
            UnsetParameter => "unset parameter",
            EmptyExpansion(error) => error.message_or_default(),
            NonassignableParameter(_) => "cannot assign to parameter",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> Cow<'_, str> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            CommandSubstError(e) => e.desc().into(),
            ArithError(e) => e.to_string().into(),
            AssignReadOnly(e) => e.to_string().into(),
            UnsetParameter => "unset parameter disallowed by the nounset option".into(),
            EmptyExpansion(e) => e.state.description().into(),
            NonassignableParameter(e) => e.to_string().into(),
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
            ArithError(e) => e.related_location(),
            AssignReadOnly(e) => Some((
                &e.read_only_location,
                "the variable was made read-only here",
            )),
            UnsetParameter => None,
            EmptyExpansion(_) => None,
            NonassignableParameter(_) => None,
        }
    }
}

/// Explanation of an expansion failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{cause}")]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

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

/// Expands a text to a string.
///
/// This function performs the initial expansion, quote removal, and attribute
/// stripping.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
pub async fn expand_text(
    env: &mut yash_env::Env,
    text: &Text,
) -> Result<(String, Option<ExitStatus>)> {
    let mut env = initial::Env::new(env);
    // It would be technically correct to set `will_split` to false, but it does
    // not affect the final results because we will join the results anyway.
    // env.will_split = false;
    let phrase = expand(&mut env, text).await?;
    let chars = phrase.ifs_join(&env.inner.variables);
    let result = skip_quotes(chars).strip().collect();
    Ok((result, env.last_command_subst_exit_status))
}

/// Expands a word to an attributed field.
///
/// This function performs initial expansion and joins the resultant phrase into
/// a field. The second field of the result tuple is the exit status of the last
/// command substitution performed during the expansion, if any.
///
/// Compare [`expand_word`] that performs not only initial expansion but also
/// quote removal and attribute stripping.
pub async fn expand_word_attr(
    env: &mut yash_env::Env,
    word: &Word,
) -> Result<(AttrField, Option<ExitStatus>)> {
    let mut env = initial::Env::new(env);
    // It would be technically correct to set `will_split` to false, but it does
    // not affect the final results because we will join the results anyway.
    // env.will_split = false;
    let phrase = expand(&mut env, word).await?;
    let chars = phrase.ifs_join(&env.inner.variables);
    let origin = word.location.clone();
    let field = AttrField { chars, origin };
    Ok((field, env.last_command_subst_exit_status))
}

/// Expands a word to a field.
///
/// This function performs the initial expansion, quote removal, and attribute
/// stripping.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
///
/// To expand a word to an [`AttrField`] without performing quote removal or
/// attribute stripping, use [`expand_word_attr`].
/// To expand multiple words to multiple fields, use [`expand_words`].
pub async fn expand_word(
    env: &mut yash_env::Env,
    word: &Word,
) -> Result<(Field, Option<ExitStatus>)> {
    let (field, exit_status) = expand_word_attr(env, word).await?;
    let field = field.remove_quotes_and_strip();
    Ok((field, exit_status))
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
    let mut env = initial::Env::new(env);

    // initial expansion //
    let words = words.into_iter();
    let mut fields = Vec::with_capacity(words.size_hint().0);
    for word in words {
        let phrase = expand(&mut env, word).await?;
        fields.extend(phrase.into_iter().map(|chars| AttrField {
            chars,
            origin: word.location.clone(),
        }));
    }

    // TODO brace expansion

    // field splitting //
    use yash_env::variable::Value::Scalar;
    #[rustfmt::skip]
    let ifs = match env.inner.variables.get("IFS") {
        Some(&Variable { value: Some(Scalar(ref value)), ..  }) => Ifs::new(value),
        // TODO If the variable is an array, should we ignore it?
        _ => Ifs::default(),
    };
    let mut split_fields = Vec::with_capacity(fields.len());
    for field in fields {
        split::split_into(field, &ifs, &mut split_fields);
    }
    drop(ifs);

    // pathname expansion (including quote removal and attribute stripping) //
    let mut fields = Vec::with_capacity(split_fields.len());
    for field in split_fields {
        fields.extend(glob(env.inner, field));
    }
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
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::num::NonZeroU64;
    use std::rc::Rc;
    use yash_env::variable::Scope;
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
        let new_value = Variable::new("value").set_assigned_location(Location::dummy("assigned"));
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
    fn expand_words_performs_initial_expansion() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let words = &["[$(echo echoed; return -n 42)]".parse().unwrap()];
            let (fields, exit_status) = expand_words(&mut env, words).await.unwrap();
            assert_eq!(exit_status, Some(ExitStatus(42)));
            assert_matches!(fields.as_slice(), [f] => {
                assert_eq!(f.value, "[echoed]");
            });
        })
    }

    #[test]
    fn expand_words_performs_field_splitting_possibly_with_default_ifs() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(Scope::Global, "v".to_string(), Variable::new("foo  bar "))
            .unwrap();
        let words = &["$v".parse().unwrap()];
        let result = expand_words(&mut env, words).now_or_never().unwrap();
        let (fields, exit_status) = result.unwrap();
        assert_eq!(exit_status, None);
        assert_matches!(fields.as_slice(), [f1, f2] => {
            assert_eq!(f1.value, "foo");
            assert_eq!(f2.value, "bar");
        });
    }

    #[test]
    fn expand_words_performs_field_splitting_with_current_ifs() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .assign(Scope::Global, "v".to_string(), Variable::new("foo  bar "))
            .unwrap();
        env.variables
            .assign(Scope::Global, "IFS".to_string(), Variable::new(" o"))
            .unwrap();
        let words = &["$v".parse().unwrap()];
        let result = expand_words(&mut env, words).now_or_never().unwrap();
        let (fields, exit_status) = result.unwrap();
        assert_eq!(exit_status, None);
        assert_matches!(fields.as_slice(), [f1, f2, f3] => {
            assert_eq!(f1.value, "f");
            assert_eq!(f2.value, "");
            assert_eq!(f3.value, "bar");
        });
    }

    #[test]
    fn expand_words_performs_quote_removal() {
        let mut env = yash_env::Env::new_virtual();
        let words = &["\"foo\"'$v'".parse().unwrap()];
        let result = expand_words(&mut env, words).now_or_never().unwrap();
        let (fields, exit_status) = result.unwrap();
        assert_eq!(exit_status, None);
        assert_matches!(fields.as_slice(), [f] => {
            assert_eq!(f.value, "foo$v");
        });
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
        let result = expand_value(&mut env, &value).now_or_never().unwrap();
        let (result, exit_status) = result.unwrap();
        let content = assert_matches!(result, yash_env::variable::Array(content) => content);
        assert_eq!(content, ["".to_string(), r"2\".to_string()]);
        assert_eq!(exit_status, None);
    }
}
