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
//! `Expand`, you first create an [`Expander`] by providing an [`Env`] and
//! [`Expansion`] implementors and then call [`expand`](Expand::expand) on the
//! text/word. If successful, the `Expansion` implementor will contain the
//! result.
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

mod quote_removal;
mod text;
mod word;

use async_trait::async_trait;
use std::ops::Deref;
use std::ops::DerefMut;
use yash_syntax::source::Location;
use yash_syntax::syntax::Word;

#[doc(no_inline)]
pub use yash_env::expansion::*;

pub use quote_removal::*;

/// Types of errors that may occur in the word expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    // TODO Define error cause types
}

/// Explanation of an expansion failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

/// Result of word expansion.
///
/// Because fields resulting from the expansion are stored in the [`Expander`],
/// the OK value of the result is usually `()`.
pub type Result<T = ()> = std::result::Result<T, Error>;

/// Part of the shell execution environment the word expansion depends on.
pub trait Env: std::fmt::Debug {
    // TODO define Env methods
}
// TODO Should we split Env for the initial expansion and multi-field expansion?

impl Env for yash_env::Env {
    // TODO implement Env methods for yash_env::Env
}

/// Origin of a character produced in the initial expansion.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Origin {
    /// The character appeared literally in the original word.
    Literal,
    /// The character originates from a tilde expansion or sequencing brace expansion.
    ///
    /// This kind of character is treated literally in the pathname expansion.
    HardExpansion,
    /// The character originates from a parameter expansion, command substitution, or arithmetic expansion.
    ///
    /// This kind of character is subject to field splitting where applicable.
    SoftExpansion,
}

/// Character with attributes describing its origin.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AttrChar {
    /// Character value.
    pub value: char,
    /// Character origin.
    pub origin: Origin,
    /// Whether this character is quoted by another character.
    pub is_quoted: bool,
    /// Whether this is a quotation character that quotes another character.
    ///
    /// Note that a character can be both quoting and quoted. For example, the
    /// backslash in `"\$"` quotes the dollar and is quoted by the
    /// double-quotes.
    pub is_quoting: bool,
}

/// Result of the initial expansion.
///
/// An `AttrField` is a string of `AttrChar`s with the location of the word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttrField {
    /// Value of the field.
    pub chars: Vec<AttrChar>,
    /// Location of the word this field resulted from.
    pub origin: Location,
}

/// Interface to accumulate results of the initial expansion.
///
/// `Expansion` is implemented by types that can accumulate [`AttrChar`]s or
/// vectors of them. You construct an [`Expander`] using an `Expansion`
/// implementor and then use it to carry out the initial expansion.
pub trait Expansion: std::fmt::Debug {
    /// Appends a character to the current field.
    fn push_char(&mut self, c: AttrChar);

    /// Appends characters to the current field.
    ///
    /// The appended characters share the same `origin`, `is_quoted`, and
    /// `is_quoting` attributes.
    fn push_str(&mut self, s: &str, origin: Origin, is_quoted: bool, is_quoting: bool) {
        for c in s.chars() {
            self.push_char(AttrChar {
                value: c,
                origin,
                is_quoted,
                is_quoting,
            });
        }
    }
}
// TODO impl Expansion::push_fields

/// Produces a single field as a result of the expansion.
impl Expansion for Vec<AttrChar> {
    fn push_char(&mut self, c: AttrChar) {
        self.push(c)
    }
}

/// Produces any number of fields as a result of the expansion.
impl Expansion for Vec<Vec<AttrChar>> {
    fn push_char(&mut self, c: AttrChar) {
        if let Some(field) = self.last_mut() {
            field.push(c);
        } else {
            self.push(vec![c]);
        }
    }
}

/// Shell execution environment for performing the initial expansion in.
///
/// An expander is a collection of data used in the initial expansion.
/// It contains a reference to implementors of [`Env`] and [`Expansion`].
#[derive(Debug)]
pub struct Expander<'e, E: Env> {
    /// Environment used in the word expansion.
    env: &'e mut E,
    /// Fields resulting from the initial expansion.
    result: &'e mut dyn Expansion,
    /// Whether the currently expanded part is double-quoted.
    is_quoted: bool,
}

impl<'e, E: Env> Expander<'e, E> {
    /// Creates a new expander.
    ///
    /// This function requires two parameters:
    ///
    /// - `env`: An environment in which the expansion is performed.
    /// - `result`: An implementor of `Expansion` into which the expansion
    ///   results are inserted.
    pub fn new(env: &'e mut E, result: &'e mut dyn Expansion) -> Self {
        Expander {
            env,
            result,
            is_quoted: false,
        }
    }

    /// Whether the currently expanded part is quoted.
    ///
    /// By default, this function returns `false`. If you [begin a
    /// quotation](Self::begin_quote), it will return `true` until you [end the
    /// quotation](Self::end_quote).
    pub fn is_quoted(&self) -> bool {
        self.is_quoted
    }
}

impl<E: Env> Deref for Expander<'_, E> {
    type Target = E;
    fn deref(&self) -> &E {
        self.env
    }
}

impl<E: Env> DerefMut for Expander<'_, E> {
    fn deref_mut(&mut self) -> &mut E {
        self.env
    }
}

/// The `Expansion` implementation for `Expander` delegates to the `Expansion`
/// implementor contained in the `Expander`.
impl<E: Env> Expansion for Expander<'_, E> {
    fn push_char(&mut self, c: AttrChar) {
        self.result.push_char(c)
    }
}

/// RAII-style guard for temporarily setting [`Expander::is_quoted`] to `true`.
///
/// When the instance of `QuotedExpander` is dropped, `is_quoted` is reset to
/// the previous value.
#[derive(Debug)]
pub struct QuotedExpander<'q, 'e, E: Env> {
    /// The expander
    expander: &'q mut Expander<'e, E>,
    /// Previous value of `is_quoted`.
    was_quoted: bool,
}

impl<'q, 'e, E: Env> Drop for QuotedExpander<'q, 'e, E> {
    /// Resets `is_quoted` of the expander to the previous value.
    fn drop(&mut self) {
        self.expander.is_quoted = self.was_quoted;
    }
}

impl<'q, 'e, E: Env> Deref for QuotedExpander<'q, 'e, E> {
    type Target = Expander<'e, E>;
    fn deref(&self) -> &Expander<'e, E> {
        self.expander
    }
}

impl<'q, 'e, E: Env> DerefMut for QuotedExpander<'q, 'e, E> {
    fn deref_mut(&mut self) -> &mut Expander<'e, E> {
        self.expander
    }
}

impl<'e, E: Env> Expander<'e, E> {
    /// Sets `is_quoted` to true.
    ///
    /// This function returns an instance of `QuotedExpander` that borrows
    /// `self`. As an implementor of `Deref` and `DerefMut`, it allows you to
    /// access the original expander. When the `QuotedExpander` is dropped or
    /// passed to [`end_quote`](Self::end_quote), `is_quoted` is reset to the
    /// previous value.
    ///
    /// This function does not directly affect `is_quoted` of [`AttrChar`]s
    /// pushed to the [`Expansion`].
    pub fn begin_quote(&mut self) -> QuotedExpander<'_, 'e, E> {
        let was_quoted = std::mem::replace(&mut self.is_quoted, true);
        QuotedExpander {
            expander: self,
            was_quoted,
        }
    }

    /// Resets `is_quoted` to the previous value.
    ///
    /// This function is equivalent to dropping the `QuotedExpander` instance
    /// but allows more descriptive code.
    pub fn end_quote(_: QuotedExpander<'_, 'e, E>) {}
}

/// Syntactic construct that can be subjected to the word expansion.
///
/// Implementors of this trait expand themselves to an [`Expander`].
/// See also [`ExpandToField`].
#[async_trait(?Send)]
pub trait Expand {
    /// Performs the initial expansion.
    ///
    /// The results should be pushed to the expander.
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result;
}

#[async_trait(?Send)]
impl<T: Expand> Expand for [T] {
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result {
        for item in self {
            item.expand(e).await?;
        }
        Ok(())
    }
}

/// Syntactic construct that can be expanded to an [`AttrField`].
///
/// Implementors of this trait expand themselves directly to an `AttrField` or
/// a vector of `AttrField`s. See also [`Expand`].
#[async_trait(?Send)]
pub trait ExpandToField {
    /// Performs the initial expansion on `self`, producing a single field.
    ///
    /// This is usually used in contexts where field splitting will not be
    /// performed on the result.
    async fn expand_to_field<E: Env>(&self, env: &mut E) -> Result<AttrField>;

    /// Performs the initial expansion on `self`, producing any number of
    /// fields.
    ///
    /// This is usually used in contexts where field splitting will be performed
    /// on the result.
    ///
    /// This function inserts the results into `fields`.
    /// See also [`expand_to_fields`](Self::expand_to_fields).
    async fn expand_to_fields_into<E: Env, F: Extend<AttrField>>(
        &self,
        env: &mut E,
        fields: &mut F,
    ) -> Result;

    /// Performs the initial expansion on `self`, producing any number of
    /// fields.
    ///
    /// This is usually used in contexts where field splitting will be performed
    /// on the result.
    ///
    /// This function returns a vector of resultant fields.
    /// See also [`expand_to_fields_into`](Self::expand_to_fields_into).
    async fn expand_to_fields<E: Env>(&self, env: &mut E) -> Result<Vec<AttrField>> {
        let mut fields = Vec::new();
        self.expand_to_fields_into(env, &mut fields).await?;
        Ok(fields)
    }
}

/// Expands words to fields.
///
/// This function performs all of the initial expansion, multi-field expansion,
/// and quote removal.
pub async fn expand_words<'a, E, I>(env: &mut E, words: I) -> Result<Vec<Field>>
where
    E: Env,
    I: IntoIterator<Item = &'a Word>,
{
    let mut fields = Vec::new();
    for word in words {
        word.expand_to_fields_into(env, &mut fields).await?;
    }
    // TODO multi-field expansion
    Ok(fields
        .into_iter()
        .map(QuoteRemoval::do_quote_removal)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct NullEnv;

    impl Env for NullEnv {}

    #[test]
    fn expansion_push_str() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let to = AttrChar {
            value: '-',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let z = AttrChar {
            value: 'z',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };

        let mut field = Vec::<AttrChar>::default();
        field.push_str("a-z", Origin::SoftExpansion, true, false);
        assert_eq!(field, [a, to, z]);
    }

    #[test]
    fn attr_field_push_char() {
        let c = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let d = AttrChar {
            value: 'Y',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let mut field = Vec::<AttrChar>::default();
        field.push_char(c);
        assert_eq!(field, [c]);
        field.push_char(d);
        assert_eq!(field, [c, d]);
    }

    #[test]
    fn vec_attr_field_push_char() {
        let c = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let d = AttrChar {
            value: 'Y',
            origin: Origin::HardExpansion,
            is_quoted: false,
            is_quoting: true,
        };
        let mut fields = Vec::<Vec<AttrChar>>::default();
        fields.push_char(c);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], [c]);
        fields.push_char(d);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], [c, d]);
    }

    // TODO Test Vec<Vec<AttrChar>>::push_char with multiple existing fields

    #[allow(clippy::bool_assert_comparison)]
    #[test]
    fn quoted_expander() {
        let mut field = Vec::<AttrChar>::default();
        let mut env = NullEnv;
        let mut expander = Expander::new(&mut env, &mut field);
        assert_eq!(expander.is_quoted(), false);
        {
            let mut expander = expander.begin_quote();
            assert_eq!(expander.is_quoted(), true);
            {
                let expander = expander.begin_quote();
                assert_eq!(expander.is_quoted(), true);
                Expander::end_quote(expander);
            }
            assert_eq!(expander.is_quoted(), true);
            Expander::end_quote(expander);
        }
        assert_eq!(expander.is_quoted(), false);
    }
}
