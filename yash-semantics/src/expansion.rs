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
//! # Initial expansion
//!
//! TODO Elaborate: Tilde expansion, parameter expansion, command substitution,
//! and arithmetic expansion.
//!
//! # Multi-field expansion
//!
//! TODO Elaborate: Brace expansion, field splitting, pathname expansion, empty
//! field removal.
//!
//! # Quote removal
//!
//! TODO Elaborate

use async_trait::async_trait;
use std::ops::Deref;
use std::ops::DerefMut;
use yash_syntax::source::Location;

#[doc(no_inline)]
pub use yash_env::expansion::*;

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
    /// backslash in the word `"\$"` quotes the dollar and is quoted by the
    /// double-quotes.
    pub is_quoting: bool,
}

/// Result of the initial expansion.
///
/// An `AttrField` is a string of `AttrChar`s.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AttrField(pub Vec<AttrChar>);
/*
pub struct AttrField {
    /// Value of the field.
    pub value: Vec<AttrChar>,
    /// Location of the word this field resulted from.
    pub origin: Location,
}
*/

/// Result of the initial expansion.
pub trait Expansion: std::fmt::Debug {}
// TODO impl Expansion::push_char
// TODO impl Expansion::push_fields

/// Result of the initial expansion for expanding to a single field.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Single(pub AttrField);

/// Result of the initial expansion for subjecting to field splitting.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Multiple(pub Vec<AttrField>);
// TODO Consider using smallvec instead of Vec

/// Shell execution environment for performing the word expansion in.
///
/// TODO Elaborate
#[derive(Debug)]
pub struct Expander<'e, E: Env> {
    /// Environment used in the word expansion.
    env: &'e mut E,
    /// Fields resulting from the word expansion.
    result: &'e mut dyn Expansion,
    // TODO inside double-quotes?
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
        Expander { env, result }
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

impl<E: Env> Expansion for Expander<'_, E> {}

/// Syntactic construct that can be subjected to the word expansion.
#[async_trait(?Send)]
pub trait Word {
    /// Performs the word expansion.
    ///
    /// The results should be pushed to the expander.
    async fn expand<E: Env>(&self, e: &mut Expander<'_, E>) -> Result;
}
