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
//! The [`expand_word_multiple`] function performs all of them and produces
//! any number of fields depending on the expanded word. The [`expand_word_attr`]
//! and [`expand_word`] functions omit some of them to ensure that the result is
//! a single field. Other functions in this module are provided for convenience
//! in specific situations.
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
//! (TODO: This feature is not yet implemented.)
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
//! [`AttrField`]s into bare [`Field`]s. In [`expand_word_multiple`], the quote
//! removal is performed between the field splitting and pathname expansion, and
//! the attribute stripping is part of the pathname expansion. In
//! [`expand_word`], they are carried out as the last step of the whole
//! expansion.

pub(crate) mod attr_fnmatch;
pub mod glob;
pub mod initial;
pub mod phrase;

use self::attr::AttrChar;
use self::attr::AttrField;
use self::attr::Origin;
use self::attr_strip::Strip;
use self::glob::glob;
use self::initial::ArithError;
#[cfg(doc)]
use self::initial::Expand;
use self::initial::Expand as _;
use self::initial::NonassignableError;
use self::initial::Vacancy;
use self::initial::VacantError;
use self::quote_removal::skip_quotes;
use self::split::Ifs;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_env::system::System;
use yash_env::variable::IFS;
use yash_env::variable::Value;
use yash_syntax::source::Location;
use yash_syntax::source::pretty::Footnote;
use yash_syntax::source::pretty::FootnoteType;
use yash_syntax::source::pretty::Report;
use yash_syntax::source::pretty::ReportType;
use yash_syntax::source::pretty::Snippet;
use yash_syntax::source::pretty::Span;
use yash_syntax::source::pretty::SpanRole;
use yash_syntax::source::pretty::add_span;
use yash_syntax::syntax::ExpansionMode;
use yash_syntax::syntax::Param;
use yash_syntax::syntax::Text;
use yash_syntax::syntax::Word;

#[doc(no_inline)]
pub use yash_env::semantics::Field;
#[doc(no_inline)]
pub use yash_env::semantics::expansion::{attr, attr_strip, quote_removal, split};

/// Error returned on assigning to a read-only variable
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cannot assign to read-only variable {name:?}")]
pub struct AssignReadOnlyError {
    /// Name of the read-only variable
    pub name: String,
    /// Value that was being assigned
    pub new_value: Value,
    /// Location where the variable was made read-only
    pub read_only_location: Location,
    /// State of the variable before the assignment
    ///
    /// If this assignment error occurred in a parameter expansion as in
    /// `${foo=bar}` or `${foo:=bar}`, this field is `Some`, and the value is
    /// the state of the variable before the assignment. In other cases, this
    /// field is `None`.
    pub vacancy: Option<Vacancy>,
}

/// Types of errors that may occur in the word expansion.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ErrorCause {
    /// System error while performing a command substitution.
    #[error("error in command substitution: {0}")]
    CommandSubstError(Errno),

    /// Error while evaluating an arithmetic expansion.
    #[error(transparent)]
    ArithError(#[from] ArithError),

    /// Assignment to a read-only variable.
    #[error(transparent)]
    AssignReadOnly(#[from] AssignReadOnlyError),

    /// Expansion of an unset parameter with the `nounset` option
    #[error("unset parameter `{param}`")]
    UnsetParameter { param: Param },

    /// Expansion of an empty value with an error switch
    #[error(transparent)]
    VacantExpansion(#[from] VacantError),

    /// Assignment to a nonassignable parameter
    #[error(transparent)]
    NonassignableParameter(#[from] NonassignableError),
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
            AssignReadOnly(_) => "error assigning to variable",
            UnsetParameter { .. } => "cannot expand unset parameter",
            VacantExpansion(error) => error.message_or_default(),
            NonassignableParameter(_) => "cannot assign to parameter",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> Cow<'_, str> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            CommandSubstError(e) => e.to_string(),
            ArithError(e) => e.to_string(),
            AssignReadOnly(e) => e.to_string(),
            UnsetParameter { param } => format!("parameter `{param}` is not set"),
            VacantExpansion(e) => match e.vacancy {
                Vacancy::Unset => format!("parameter `{}` is not set", e.param),
                Vacancy::EmptyScalar => format!("parameter `{}` is an empty string", e.param),
                Vacancy::ValuelessArray => format!("parameter `{}` is an empty array", e.param),
                Vacancy::EmptyValueArray => {
                    format!("parameter `{}` is an array of an empty string", e.param)
                }
            },
            NonassignableParameter(e) => e.to_string(),
        }
        .into()
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
            UnsetParameter { .. } => None,
            VacantExpansion(_) => None,
            NonassignableParameter(_) => None,
        }
    }

    /// Returns a footer message for the error.
    #[must_use]
    pub fn footer(&self) -> Option<&'static str> {
        use ErrorCause::*;
        match self {
            CommandSubstError(_)
            | ArithError(_)
            | AssignReadOnly(_)
            | VacantExpansion(_)
            | NonassignableParameter(_) => None,

            UnsetParameter { .. } => Some("unset parameters are disallowed by the nounset option"),
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

impl Error {
    /// Returns a report for the error.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.cause.message().into();
        report.snippets = Snippet::with_primary_span(&self.location, self.cause.label());

        if let Some((location, label)) = self.cause.related_location() {
            let label = label.into();
            let span = Span {
                range: location.byte_range(),
                role: SpanRole::Supplementary { label },
            };
            add_span(&location.code, span, &mut report.snippets);
        }

        if let Some(footer) = self.cause.footer() {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: footer.into(),
            });
        }

        // Report the vacancy that caused the assignment that led to the error.
        let vacancy = match &self.cause {
            ErrorCause::CommandSubstError(_) => None,
            ErrorCause::ArithError(_) => None,
            ErrorCause::AssignReadOnly(e) => e.vacancy,
            ErrorCause::UnsetParameter { .. } => None,
            ErrorCause::VacantExpansion(_) => None,
            ErrorCause::NonassignableParameter(e) => Some(e.vacancy),
        };
        if let Some(vacancy) = vacancy {
            let message = match vacancy {
                Vacancy::Unset => "assignment was attempted because the parameter was not set",
                Vacancy::EmptyScalar => {
                    "assignment was attempted because the parameter was an empty string"
                }
                Vacancy::ValuelessArray => {
                    "assignment was attempted because the parameter was an empty array"
                }
                Vacancy::EmptyValueArray => {
                    "assignment was attempted because the parameter was an array of an empty string"
                }
            };
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: message.into(),
            });
        }

        report
    }
}

/// Converts the error into a report by calling [`Error::to_report`].
impl<'a> From<&'a Error> for Report<'a> {
    #[inline(always)]
    fn from(error: &'a Error) -> Self {
        error.to_report()
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
pub async fn expand_text<S: System + 'static>(
    env: &mut yash_env::Env<S>,
    text: &Text,
) -> Result<(String, Option<ExitStatus>)> {
    let mut env = initial::Env::new(env);
    // It would be technically correct to set `will_split` to false, but it does
    // not affect the final results because we will join the results anyway.
    // env.will_split = false;
    let phrase = text.expand(&mut env).await?;
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
pub async fn expand_word_attr<S: System + 'static>(
    env: &mut yash_env::Env<S>,
    word: &Word,
) -> Result<(AttrField, Option<ExitStatus>)> {
    let mut env = initial::Env::new(env);
    // It would be technically correct to set `will_split` to false, but it does
    // not affect the final results because we will join the results anyway.
    // env.will_split = false;
    let phrase = word.expand(&mut env).await?;
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
/// To expand a word to multiple fields, use [`expand_word_multiple`].
/// To expand multiple words to multiple fields, use [`expand_words`].
pub async fn expand_word<S: System + 'static>(
    env: &mut yash_env::Env<S>,
    word: &Word,
) -> Result<(Field, Option<ExitStatus>)> {
    let (field, exit_status) = expand_word_attr(env, word).await?;
    let field = field.remove_quotes_and_strip();
    Ok((field, exit_status))
}

/// Expands a word to fields.
///
/// This function performs the initial expansion and multi-field expansion,
/// including quote removal and attribute stripping. The results are appended to
/// the given collection. The return value is the exit status of the last
/// command substitution performed during the expansion, if any.
///
/// To expand a single word to a single field, use [`expand_word`].
/// To expand multiple words to fields, use [`expand_words`].
pub async fn expand_word_multiple<S, R>(
    env: &mut yash_env::Env<S>,
    word: &Word,
    results: &mut R,
) -> Result<Option<ExitStatus>>
where
    S: System + 'static,
    R: Extend<Field>,
{
    let mut env = initial::Env::new(env);

    // initial expansion //
    let phrase = word.expand(&mut env).await?;

    // TODO brace expansion //

    // field splitting //
    let ifs = env
        .inner
        .variables
        .get_scalar(IFS)
        .map(Ifs::new)
        .unwrap_or_default();
    let mut split_fields = Vec::with_capacity(phrase.field_count());
    for chars in phrase {
        let origin = word.location.clone();
        let attr_field = AttrField { chars, origin };
        split::split_into(attr_field, &ifs, &mut split_fields);
    }
    drop(ifs);

    // pathname expansion (including quote removal and attribute stripping) //
    for field in split_fields {
        results.extend(glob(env.inner, field));
    }

    Ok(env.last_command_subst_exit_status)
}

/// Expands a word to fields.
///
/// This function expands a word to fields using the specified expansion mode
/// and appends the results to the given collection.
///
/// If the specified mode is [`ExpansionMode::Multiple`], this function performs
/// the initial expansion and multi-field expansion, including quote removal and
/// attribute stripping (see [`expand_word_multiple`]). If the mode is
/// [`ExpansionMode::Single`], this function performs the initial expansion,
/// quote removal, and attribute stripping, but not multi-field expansion (see
/// [`expand_word`]).
///
/// The results are appended to the given collection.
pub async fn expand_word_with_mode<S, R>(
    env: &mut yash_env::Env<S>,
    word: &Word,
    mode: ExpansionMode,
    results: &mut R,
) -> Result<Option<ExitStatus>>
where
    S: System + 'static,
    R: Extend<Field>,
{
    match mode {
        ExpansionMode::Single => {
            let (field, exit_status) = expand_word(env, word).await?;
            results.extend(std::iter::once(field));
            Ok(exit_status)
        }
        ExpansionMode::Multiple => expand_word_multiple(env, word, results).await,
    }
}

/// Expands words to fields.
///
/// This function performs the initial expansion and multi-field expansion,
/// including quote removal and attribute stripping.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
///
/// To expand a single word to a single field, use [`expand_word`].
/// To expand a single word to multiple fields, use [`expand_word_multiple`].
pub async fn expand_words<'a, S, I>(
    env: &mut yash_env::Env<S>,
    words: I,
) -> Result<(Vec<Field>, Option<ExitStatus>)>
where
    S: System + 'static,
    I: IntoIterator<Item = &'a Word>,
{
    let mut fields = Vec::new();
    let mut last_exit_status = None;

    for word in words {
        let exit_status = expand_word_multiple(env, word, &mut fields).await?;
        if exit_status.is_some() {
            last_exit_status = exit_status;
        }
    }

    Ok((fields, last_exit_status))
}

/// Expands an assignment value.
///
/// This function expands a [`yash_syntax::syntax::Value`] to a
/// [`yash_env::variable::Value`]. A scalar and array value are expanded by
/// [`expand_word`] and [`expand_words`], respectively.
/// The second field of the result tuple is the exit status of the last command
/// substitution performed during the expansion, if any.
pub async fn expand_value<S: System + 'static>(
    env: &mut yash_env::Env<S>,
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
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use yash_env::variable::Scope;
    use yash_env_test_helper::in_virtual_system;

    #[test]
    fn from_error_for_report() {
        let error = Error {
            cause: ErrorCause::AssignReadOnly(AssignReadOnlyError {
                name: "foo".into(),
                new_value: "value".into(),
                read_only_location: Location::dummy("ROL"),
                vacancy: None,
            }),
            location: Location {
                range: 2..4,
                ..Location::dummy("hello")
            },
        };

        let report = Report::from(&error);

        assert_eq!(report.r#type, ReportType::Error);
        assert_eq!(report.title, "error assigning to variable");
        assert_eq!(report.snippets.len(), 2);
        assert_eq!(*report.snippets[0].code.value.borrow(), "hello");
        assert_eq!(report.snippets[0].spans.len(), 1);
        assert_eq!(report.snippets[0].spans[0].range, 2..4);
        assert_matches!(
            &report.snippets[0].spans[0].role,
            SpanRole::Primary { label } if label == "cannot assign to read-only variable \"foo\""
        );
        assert_eq!(*report.snippets[1].code.value.borrow(), "ROL");
        assert_eq!(report.snippets[1].spans.len(), 1);
        assert_eq!(report.snippets[1].spans[0].range, 0..3);
        assert_matches!(
            &report.snippets[1].spans[0].role,
            SpanRole::Supplementary { label } if label == "the variable was made read-only here"
        );
        assert_eq!(report.footnotes, []);
    }

    #[test]
    fn from_error_for_report_with_vacancy() {
        let error = Error {
            cause: ErrorCause::AssignReadOnly(AssignReadOnlyError {
                name: "foo".into(),
                new_value: "value".into(),
                read_only_location: Location::dummy("ROL"),
                vacancy: Some(Vacancy::EmptyScalar),
            }),
            location: Location {
                range: 2..4,
                ..Location::dummy("hello")
            },
        };

        let report = Report::from(&error);

        assert_eq!(
            report.footnotes,
            [Footnote {
                r#type: FootnoteType::Note,
                label: "assignment was attempted because the parameter was an empty string".into(),
            }]
        );
    }

    #[test]
    fn expand_word_multiple_performs_initial_expansion() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let word = "[$(echo echoed; return -n 42)]".parse().unwrap();
            let mut fields = Vec::new();
            let exit_status = expand_word_multiple(&mut env, &word, &mut fields)
                .await
                .unwrap();
            assert_eq!(exit_status, Some(ExitStatus(42)));
            assert_matches!(fields.as_slice(), [f] => {
                assert_eq!(f.value, "[echoed]");
            });
        })
    }

    #[test]
    fn expand_word_multiple_performs_field_splitting_possibly_with_default_ifs() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .get_or_new("v", Scope::Global)
            .assign("foo  bar ", None)
            .unwrap();
        let word = "$v".parse().unwrap();
        let mut fields = Vec::new();
        let exit_status = expand_word_multiple(&mut env, &word, &mut fields)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        assert_matches!(fields.as_slice(), [f1, f2] => {
            assert_eq!(f1.value, "foo");
            assert_eq!(f2.value, "bar");
        });
    }

    #[test]
    fn expand_word_multiple_performs_field_splitting_with_current_ifs() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .get_or_new("v", Scope::Global)
            .assign("foo  bar ", None)
            .unwrap();
        env.variables
            .get_or_new(IFS, Scope::Global)
            .assign(" o", None)
            .unwrap();
        let word = "$v".parse().unwrap();
        let mut fields = Vec::new();
        let exit_status = expand_word_multiple(&mut env, &word, &mut fields)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        assert_matches!(fields.as_slice(), [f1, f2, f3] => {
            assert_eq!(f1.value, "f");
            assert_eq!(f2.value, "");
            assert_eq!(f3.value, "bar");
        });
    }

    #[test]
    fn expand_word_multiple_performs_quote_removal() {
        let mut env = yash_env::Env::new_virtual();
        let word = "\"foo\"'$v'".parse().unwrap();
        let mut fields = Vec::new();
        let exit_status = expand_word_multiple(&mut env, &word, &mut fields)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(exit_status, None);
        assert_matches!(fields.as_slice(), [f] => {
            assert_eq!(f.value, "foo$v");
        });
    }

    #[test]
    fn expand_words_returns_exit_status_of_last_command_substitution() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let word1 = "$(return -n 12)".parse().unwrap();
            let word2 = "$(return -n 34)$(return -n 56)".parse().unwrap();
            let (_, exit_status) = expand_words(&mut env, &[word1, word2]).await.unwrap();
            assert_eq!(exit_status, Some(ExitStatus(56)));
        })
    }

    #[test]
    fn expand_words_performs_field_splitting() {
        let mut env = yash_env::Env::new_virtual();
        env.variables
            .get_or_new("v", Scope::Global)
            .assign(" foo  bar ", None)
            .unwrap();
        let word = "$v".parse().unwrap();
        let (fields, _) = expand_words(&mut env, &[word])
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_matches!(fields.as_slice(), [f1, f2] => {
            assert_eq!(f1.value, "foo");
            assert_eq!(f2.value, "bar");
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
