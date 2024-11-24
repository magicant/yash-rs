// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

//! Shell command language syntax
//!
//! This module contains types that represent abstract syntax trees (ASTs) of
//! the shell language.
//!
//! ## Syntactic elements
//!
//! The AST type that represents the whole shell script is [`List`], which is a
//! vector of [`Item`]s. An `Item` is a possibly asynchronous [`AndOrList`],
//! which is a sequence of conditionally executed [`Pipeline`]s. A `Pipeline` is
//! a sequence of [`Command`]s separated by `|`.
//!
//! There are several types of `Command`s, namely [`SimpleCommand`],
//! [`CompoundCommand`] and [`FunctionDefinition`], where `CompoundCommand` in
//! turn comes in many variants.
//!
//! ## Lexical elements
//!
//! Tokens that make up commands may contain quotations and expansions. A
//! [`Word`], a sequence of [`WordUnit`]s, represents such a token that appears
//! in a simple command and some kinds of other commands.
//!
//! In some contexts, tilde expansion and single- and double-quotes are not
//! recognized while other kinds of expansions are allowed. Such part is
//! represented as [`Text`], a sequence of [`TextUnit`]s.
//!
//! ## Parsing
//!
//! Most AST types defined in this module implement the [`FromStr`] trait, which
//! means you can easily get an AST out of source code by calling `parse` on a
//! `&str`. However, all [location](crate::source::Location)s in ASTs
//! constructed this way will only have
//! [unknown source](crate::source::Source::Unknown).
//!
//! ```
//! use std::str::FromStr;
//! # use yash_syntax::syntax::List;
//! let list: List = "diff foo bar; echo $?".parse().unwrap();
//! assert_eq!(list.to_string(), "diff foo bar; echo $?");
//!
//! use yash_syntax::source::Source;
//! # use yash_syntax::syntax::Word;
//! let word: Word = "foo".parse().unwrap();
//! assert_eq!(*word.location.code.source, Source::Unknown);
//! ```
//!
//! To include substantial source information in the AST, you need to prepare a
//! [lexer](crate::parser::lex::Lexer) with source information and then use it
//! to parse the source code. See the [`parser`](crate::parser) module for
//! details.
//!
//! ## Displaying
//!
//! Most AST types support the [`Display`](std::fmt::Display) trait, which
//! allows you to convert an AST to a source code string. Note that the
//! `Display` trait implementations always produce single-line source code with
//! here-document contents omitted. To pretty-format an AST in multiple lines
//! with here-document contents included, you can use ... TODO TBD.

use crate::parser::lex::Keyword;
use crate::parser::lex::Operator;
use crate::parser::lex::TryFromOperatorError;
use crate::source::Location;
use std::cell::OnceCell;
use std::fmt;
#[cfg(unix)]
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::str::FromStr;
use thiserror::Error;

#[cfg(not(unix))]
type RawFd = i32;

/// Result of [`Unquote::write_unquoted`]
///
/// If there is some quotes to be removed, the result will be `Ok(true)`. If no
/// quotes, `Ok(false)`. On error, `Err(Error)`.
type UnquoteResult = Result<bool, fmt::Error>;

/// Removing quotes from syntax without performing expansion.
///
/// This trail will be useful only in a limited number of use cases. In the
/// normal word expansion process, quote removal is done after other kinds of
/// expansions like parameter expansion, so this trait is not used.
pub trait Unquote {
    /// Converts `self` to a string with all quotes removed and writes to `w`.
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult;

    /// Converts `self` to a string with all quotes removed.
    ///
    /// Returns a tuple of a string and a bool. The string is an unquoted version
    /// of `self`. The bool tells whether there is any quotes contained in
    /// `self`.
    fn unquote(&self) -> (String, bool) {
        let mut unquoted = String::new();
        let is_quoted = self
            .write_unquoted(&mut unquoted)
            .expect("`write_unquoted` should not fail");
        (unquoted, is_quoted)
    }
}

/// Possibly literal syntax element
///
/// A syntax element is _literal_ if it is not quoted and does not contain any
/// expansions. Such an element can be expanded to a string independently of the
/// shell execution environment.
///
/// ```
/// # use yash_syntax::syntax::MaybeLiteral;
/// # use yash_syntax::syntax::Text;
/// # use yash_syntax::syntax::TextUnit::Literal;
/// let text = Text(vec![Literal('f'), Literal('o'), Literal('o')]);
/// let expanded = text.to_string_if_literal().unwrap();
/// assert_eq!(expanded, "foo");
/// ```
///
/// ```
/// # use yash_syntax::syntax::MaybeLiteral;
/// # use yash_syntax::syntax::Text;
/// # use yash_syntax::syntax::TextUnit::Backslashed;
/// let backslashed = Text(vec![Backslashed('a')]);
/// assert_eq!(backslashed.to_string_if_literal(), None);
/// ```
pub trait MaybeLiteral {
    /// Checks if `self` is literal and, if so, converts to a string and appends
    /// it to `result`.
    ///
    /// If `self` is literal, `self` converted to a string is appended to
    /// `result` and `Ok(result)` is returned. Otherwise, `result` is not
    /// modified and `Err(result)` is returned.
    fn extend_if_literal<T: Extend<char>>(&self, result: T) -> Result<T, T>;

    /// Checks if `self` is literal and, if so, converts to a string.
    fn to_string_if_literal(&self) -> Option<String> {
        self.extend_if_literal(String::new()).ok()
    }
}

impl<T: Unquote> Unquote for [T] {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.iter()
            .try_fold(false, |quoted, item| Ok(quoted | item.write_unquoted(w)?))
    }
}

impl<T: MaybeLiteral> MaybeLiteral for [T] {
    fn extend_if_literal<R: Extend<char>>(&self, result: R) -> Result<R, R> {
        self.iter()
            .try_fold(result, |result, unit| unit.extend_if_literal(result))
    }
}

/// Special parameter
///
/// This enum value identifies a special parameter in the shell language.
/// Each special parameter is a single character that has a special meaning in
/// the shell language. For example, `@` represents all positional parameters.
///
/// See [`ParamType`] for other types of parameters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecialParam {
    /// `@` (all positional parameters)
    At,
    /// `*` (all positional parameters)
    Asterisk,
    /// `#` (number of positional parameters)
    Number,
    /// `?` (exit status of the last command)
    Question,
    /// `-` (active shell options)
    Hyphen,
    /// `$` (process ID of the shell)
    Dollar,
    /// `!` (process ID of the last asynchronous command)
    Exclamation,
    /// `0` (name of the shell or shell script)
    Zero,
}

impl SpecialParam {
    /// Returns the character representing the special parameter.
    #[must_use]
    pub const fn as_char(self) -> char {
        use SpecialParam::*;
        match self {
            At => '@',
            Asterisk => '*',
            Number => '#',
            Question => '?',
            Hyphen => '-',
            Dollar => '$',
            Exclamation => '!',
            Zero => '0',
        }
    }

    /// Returns the special parameter that corresponds to the given character.
    ///
    /// If the character does not represent any special parameter, `None` is
    /// returned.
    #[must_use]
    pub const fn from_char(c: char) -> Option<SpecialParam> {
        use SpecialParam::*;
        match c {
            '@' => Some(At),
            '*' => Some(Asterisk),
            '#' => Some(Number),
            '?' => Some(Question),
            '-' => Some(Hyphen),
            '$' => Some(Dollar),
            '!' => Some(Exclamation),
            '0' => Some(Zero),
            _ => None,
        }
    }
}

/// Error that occurs when a character cannot be parsed as a special parameter
///
/// This error value is returned by the `TryFrom<char>` and `FromStr`
/// implementations for [`SpecialParam`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("not a special parameter")]
pub struct NotSpecialParam;

impl TryFrom<char> for SpecialParam {
    type Error = NotSpecialParam;
    fn try_from(c: char) -> Result<SpecialParam, NotSpecialParam> {
        SpecialParam::from_char(c).ok_or(NotSpecialParam)
    }
}

impl FromStr for SpecialParam {
    type Err = NotSpecialParam;
    fn from_str(s: &str) -> Result<SpecialParam, NotSpecialParam> {
        // If `s` contains a single character and nothing else, parse it as a
        // special parameter.
        let mut chars = s.chars();
        chars
            .next()
            .filter(|_| chars.as_str().is_empty())
            .and_then(SpecialParam::from_char)
            .ok_or(NotSpecialParam)
    }
}

/// Type of a parameter
///
/// This enum distinguishes three types of [parameters](Param): named, special and
/// positional parameters. However, this value does not include the actual
/// parameter name as a string. The actual name is stored in a separate field in
/// the AST node that contains this value.
///
/// Note the careful use of the term "name" here. In POSIX terminology, a
/// "name" identifies a named parameter (that is, a variable) and does not
/// include special or positional parameters. An identifier that refers to any
/// kind of parameter is called a "parameter".
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ParamType {
    /// Named parameter
    Variable,
    /// Special parameter
    Special(SpecialParam),
    /// Positional parameter
    ///
    /// Positional parameters are indexed starting from 1, so the index of `0`
    /// always refers to a non-existent parameter. If the string form of a
    /// positional parameter represents an index that is too large to fit in a
    /// `usize`, the index should be `usize::MAX`, which is also guaranteed to
    /// spot a non-existent parameter since a `Vec` cannot have more than
    /// `isize::MAX` elements.
    Positional(usize),
}

impl From<SpecialParam> for ParamType {
    fn from(special: SpecialParam) -> ParamType {
        ParamType::Special(special)
    }
}

/// Parameter
///
/// A parameter is an identifier that appears in a parameter expansion
/// ([`TextUnit::RawParam`] and [`BracedParam`]). There are three
/// [types](ParamType) of parameters depending on the character category of the
/// identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Param {
    /// Literal representation of the parameter name
    ///
    /// This is the raw string form of the parameter as it appears in the source
    /// code. Examples include `foo`, `@`, `#`, `0`, and `12`.
    pub id: String,

    /// Type of the parameter
    ///
    /// This precomputed value is used to optimize the evaluation of parameter
    /// expansions by avoiding the need to parse the `id` field every time.
    ///
    /// It is your responsibility to ensure that the `type` field is consistent
    /// with the `id` field. For example, if the `id` field is `"@"`, the `type`
    /// field must be `Special(At)`. The [parser](crate::parser) ensures this
    /// invariant when it constructs a `Param` value.
    pub r#type: ParamType,
}

impl Param {
    /// Constructs a `Param` value representing a named parameter.
    ///
    /// This function assumes that the argument is a valid name for a variable.
    /// The returned `Param` value will have the `Variable` type regardless of
    /// the argument.
    #[must_use]
    pub fn variable<I: Into<String>>(id: I) -> Param {
        let id = id.into();
        let r#type = ParamType::Variable;
        Param { id, r#type }
    }
}

/// Constructs a `Param` value representing a special parameter.
impl From<SpecialParam> for Param {
    fn from(special: SpecialParam) -> Param {
        Param {
            id: special.to_string(),
            r#type: special.into(),
        }
    }
}

/// Constructs a `Param` value from a positional parameter index.
impl From<usize> for Param {
    fn from(index: usize) -> Param {
        Param {
            id: index.to_string(),
            r#type: ParamType::Positional(index),
        }
    }
}

// TODO Consider implementing FromStr for Param

/// Flag that specifies how the value is substituted in a [switch](Switch)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchType {
    /// Alter an existing value, if any. (`+`)
    Alter,
    /// Substitute a missing value with a default. (`-`)
    Default,
    /// Assign a default to the variable if the value is missing. (`=`)
    Assign,
    /// Error out if the value is missing. (`?`)
    Error,
}

/// Condition that triggers a [switch](Switch)
///
/// In the lexical grammar of the shell language, a switch condition is an
/// optional colon that precedes a switch type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchCondition {
    /// Without a colon, the switch is triggered if the parameter is unset.
    Unset,
    /// With a colon, the switch is triggered if the parameter is unset or
    /// empty.
    UnsetOrEmpty,
}

/// Parameter expansion [modifier](Modifier) that conditionally substitutes the
/// value being expanded
///
/// Examples of switches include `+foo`, `:-bar` and `:=baz`.
///
/// A switch is composed of a [condition](SwitchCondition) (an optional `:`), a
/// [type](SwitchType) (one of `+`, `-`, `=` and `?`) and a [word](Word).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Switch {
    /// How the value is substituted
    pub r#type: SwitchType,
    /// Condition that determines whether the value is substituted or not
    pub condition: SwitchCondition,
    /// Word that substitutes the parameter value
    pub word: Word,
}

impl Unquote for Switch {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        write!(w, "{}{}", self.condition, self.r#type)?;
        self.word.write_unquoted(w)
    }
}

/// Flag that specifies which side of the expanded value is removed in a
/// [trim](Trim)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrimSide {
    /// Beginning of the value
    Prefix,
    /// End of the value
    Suffix,
}

/// Flag that specifies pattern matching strategy in a [trim](Trim)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrimLength {
    /// Match as small number of characters as possible.
    Shortest,
    /// Match as large number of characters as possible.
    Longest,
}

/// Parameter expansion [modifier](Modifier) that removes the beginning or end
/// of the value being expanded
///
/// Examples of trims include `#foo`, `##bar` and `%%baz*`.
///
/// A trim is composed of a side, length and pattern.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trim {
    /// Which side of the value should be removed?
    pub side: TrimSide,
    /// How long the pattern should match?
    pub length: TrimLength,
    /// Pattern to be matched with the expanded value.
    pub pattern: Word,
}

impl Unquote for Trim {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        write!(w, "{}", self.side)?;
        match self.length {
            TrimLength::Shortest => (),
            TrimLength::Longest => write!(w, "{}", self.side)?,
        }
        self.pattern.write_unquoted(w)
    }
}

/// Attribute that modifies a parameter expansion
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Modifier {
    /// No modifier
    None,
    /// `#` prefix (`${#foo}`)
    Length,
    /// `+`, `-`, `=` or `?` suffix, optionally with `:` (`${foo:-bar}`)
    Switch(Switch),
    /// `#`, `##`, `%` or `%%` suffix
    Trim(Trim),
    // TODO Subst
}

/// Parameter expansion enclosed in braces
///
/// This struct is used only for parameter expansions that are enclosed braces.
/// Expansions that are not enclosed in braces are directly encoded with
/// [`TextUnit::RawParam`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BracedParam {
    // TODO recursive expansion
    /// Parameter to be expanded
    pub param: Param,
    // TODO index
    /// Modifier
    pub modifier: Modifier,
    /// Position of this parameter expansion in the source code
    pub location: Location,
}

impl Unquote for BracedParam {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        use Modifier::*;
        match self.modifier {
            None => {
                write!(w, "${{{}}}", self.param)?;
                Ok(false)
            }
            Length => {
                write!(w, "${{#{}}}", self.param)?;
                Ok(false)
            }
            Switch(ref switch) => {
                write!(w, "${{{}", self.param)?;
                let quoted = switch.write_unquoted(w)?;
                w.write_char('}')?;
                Ok(quoted)
            }
            Trim(ref trim) => {
                write!(w, "${{{}", self.param)?;
                let quoted = trim.write_unquoted(w)?;
                w.write_char('}')?;
                Ok(quoted)
            }
        }
    }
}

/// Element of [`TextUnit::Backquote`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackquoteUnit {
    /// Literal single character
    Literal(char),
    /// Backslash-escaped single character
    Backslashed(char),
}

impl Unquote for BackquoteUnit {
    fn write_unquoted<W: std::fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            BackquoteUnit::Literal(c) => {
                w.write_char(*c)?;
                Ok(false)
            }
            BackquoteUnit::Backslashed(c) => {
                w.write_char(*c)?;
                Ok(true)
            }
        }
    }
}

/// Element of a [Text], i.e., something that can be expanded
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextUnit {
    /// Literal single character
    Literal(char),
    /// Backslash-escaped single character
    Backslashed(char),
    /// Parameter expansion that is not enclosed in braces
    RawParam {
        /// Parameter to be expanded
        param: Param,
        /// Position of this parameter expansion in the source code
        location: Location,
    },
    /// Parameter expansion that is enclosed in braces
    BracedParam(BracedParam),
    /// Command substitution of the form `$(...)`
    CommandSubst {
        /// Command string that will be parsed and executed when the command
        /// substitution is expanded
        ///
        /// This value is reference-counted so that the shell does not have to
        /// clone the entire string when it is passed to a subshell to execute
        /// the command substitution.
        content: Rc<str>,
        /// Position of this command substitution in the source code
        location: Location,
    },
    /// Command substitution of the form `` `...` ``
    Backquote {
        /// Command string that will be parsed and executed when the command
        /// substitution is expanded
        content: Vec<BackquoteUnit>,
        /// Position of this command substitution in the source code
        location: Location,
    },
    /// Arithmetic expansion
    Arith {
        /// Expression that is to be evaluated
        content: Text,
        /// Position of this arithmetic expansion in the source code
        location: Location,
    },
}

pub use TextUnit::*;

impl Unquote for TextUnit {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            Literal(c) => {
                w.write_char(*c)?;
                Ok(false)
            }
            Backslashed(c) => {
                w.write_char(*c)?;
                Ok(true)
            }
            RawParam { param, .. } => {
                write!(w, "${param}")?;
                Ok(false)
            }
            BracedParam(param) => param.write_unquoted(w),
            // We don't remove quotes contained in the commands in command
            // substitutions. Existing shells disagree with each other.
            CommandSubst { content, .. } => {
                write!(w, "$({content})")?;
                Ok(false)
            }
            Backquote { content, .. } => {
                w.write_char('`')?;
                let quoted = content.write_unquoted(w)?;
                w.write_char('`')?;
                Ok(quoted)
            }
            Arith { content, .. } => {
                w.write_str("$((")?;
                let quoted = content.write_unquoted(w)?;
                w.write_str("))")?;
                Ok(quoted)
            }
        }
    }
}

impl MaybeLiteral for TextUnit {
    /// If `self` is `Literal`, appends the character to `result` and returns
    /// `Ok(result)`. Otherwise, returns `Err(result)`.
    fn extend_if_literal<T: Extend<char>>(&self, mut result: T) -> Result<T, T> {
        if let Literal(c) = self {
            // TODO Use Extend::extend_one
            result.extend(std::iter::once(*c));
            Ok(result)
        } else {
            Err(result)
        }
    }
}

/// String that may contain some expansions
///
/// A text is a sequence of [text unit](TextUnit)s, which may contain some kinds
/// of expansions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Text(pub Vec<TextUnit>);

impl Text {
    /// Creates a text from an iterator of literal chars.
    pub fn from_literal_chars<I: IntoIterator<Item = char>>(i: I) -> Text {
        Text(i.into_iter().map(Literal).collect())
    }
}

impl Unquote for Text {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.0.write_unquoted(w)
    }
}

impl MaybeLiteral for Text {
    fn extend_if_literal<T: Extend<char>>(&self, result: T) -> Result<T, T> {
        self.0.extend_if_literal(result)
    }
}

/// Element of a [Word], i.e., text with quotes and tilde expansion
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WordUnit {
    /// Unquoted [`TextUnit`] as a word unit
    Unquoted(TextUnit),
    /// String surrounded with a pair of single quotations
    SingleQuote(String),
    /// Text surrounded with a pair of double quotations
    DoubleQuote(Text),
    /// Tilde expansion
    ///
    /// The `String` value does not contain the initial tilde.
    Tilde(String),
}

pub use WordUnit::*;

impl Unquote for WordUnit {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            Unquoted(inner) => inner.write_unquoted(w),
            SingleQuote(inner) => {
                w.write_str(inner)?;
                Ok(true)
            }
            DoubleQuote(inner) => inner.write_unquoted(w),
            Tilde(s) => {
                write!(w, "~{s}")?;
                Ok(false)
            }
        }
    }
}

impl MaybeLiteral for WordUnit {
    /// If `self` is `Unquoted(Literal(_))`, appends the character to `result`
    /// and returns `Ok(result)`. Otherwise, returns `Err(result)`.
    fn extend_if_literal<T: Extend<char>>(&self, result: T) -> Result<T, T> {
        if let Unquoted(inner) = self {
            inner.extend_if_literal(result)
        } else {
            Err(result)
        }
    }
}

/// Token that may involve expansions and quotes
///
/// A word is a sequence of [word unit](WordUnit)s. It depends on context whether
/// an empty word is valid or not. It is your responsibility to ensure a word is
/// non-empty in a context where it cannot.
///
/// The difference between words and [text](Text)s is that only words can contain
/// single- and double-quotes and tilde expansions. Compare [`WordUnit`] and [`TextUnit`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Word {
    /// Word units that constitute the word
    pub units: Vec<WordUnit>,
    /// Position of the word in the source code
    pub location: Location,
}

impl Unquote for Word {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.units.write_unquoted(w)
    }
}

impl MaybeLiteral for Word {
    fn extend_if_literal<T: Extend<char>>(&self, result: T) -> Result<T, T> {
        self.units.extend_if_literal(result)
    }
}

/// Value of an [assignment](Assign)
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Scalar value, a possibly empty word
    ///
    /// Note: Because a scalar assignment value is created from a normal command
    /// word, the location of the word in the scalar value refers to the entire
    /// assignment word rather than the assigned value.
    Scalar(Word),

    /// Array, possibly empty list of non-empty words
    ///
    /// Array assignment is a POSIXly non-portable extension.
    Array(Vec<Word>),
}

pub use Value::*;

/// Assignment word
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assign {
    /// Name of the variable to assign to
    ///
    /// In the valid assignment syntax, the name must not be empty.
    pub name: String,
    /// Value assigned to the variable
    pub value: Value,
    /// Location of the assignment word
    pub location: Location,
}

/// Fallible conversion from a word into an assignment
impl TryFrom<Word> for Assign {
    type Error = Word;
    /// Converts a word into an assignment.
    ///
    /// For a successful conversion, the word must be of the form `name=value`,
    /// where `name` is a non-empty [literal](Word::to_string_if_literal) word,
    /// `=` is an unquoted equal sign, and `value` is a word. If the input word
    /// does not match this syntax, it is returned intact in `Err`.
    fn try_from(mut word: Word) -> Result<Assign, Word> {
        if let Some(eq) = word.units.iter().position(|u| u == &Unquoted(Literal('='))) {
            if eq > 0 {
                if let Some(name) = word.units[..eq].to_string_if_literal() {
                    assert!(!name.is_empty());
                    word.units.drain(..=eq);
                    word.parse_tilde_everywhere();
                    let location = word.location.clone();
                    let value = Scalar(word);
                    return Ok(Assign {
                        name,
                        value,
                        location,
                    });
                }
            }
        }

        Err(word)
    }
}

/// File descriptor
///
/// This is the `newtype` pattern applied to [`RawFd`], which is merely a type
/// alias.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fd(pub RawFd);

impl Fd {
    /// File descriptor for the standard input
    pub const STDIN: Fd = Fd(0);
    /// File descriptor for the standard output
    pub const STDOUT: Fd = Fd(1);
    /// File descriptor for the standard error
    pub const STDERR: Fd = Fd(2);
}

impl From<RawFd> for Fd {
    fn from(raw_fd: RawFd) -> Fd {
        Fd(raw_fd)
    }
}

/// Redirection operators
///
/// This enum defines the redirection operator types except here-document and
/// process redirection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedirOp {
    /// `<` (open a file for input)
    FileIn,
    /// `<>` (open a file for input and output)
    FileInOut,
    /// `>` (open a file for output; truncate or fail if existing)
    FileOut,
    /// `>>` (open a file for output; append if existing)
    FileAppend,
    /// `>|` (open a file for output; always truncate if existing)
    FileClobber,
    /// `<&` (copy or close a file descriptor for input)
    FdIn,
    /// `>&` (copy or close a file descriptor for output)
    FdOut,
    /// `>>|` (open a pipe, one end for input and the other output)
    Pipe,
    /// `<<<` (here-string)
    String,
}

impl TryFrom<Operator> for RedirOp {
    type Error = TryFromOperatorError;
    fn try_from(op: Operator) -> Result<RedirOp, TryFromOperatorError> {
        use Operator::*;
        use RedirOp::*;
        match op {
            Less => Ok(FileIn),
            LessGreater => Ok(FileInOut),
            Greater => Ok(FileOut),
            GreaterGreater => Ok(FileAppend),
            GreaterBar => Ok(FileClobber),
            LessAnd => Ok(FdIn),
            GreaterAnd => Ok(FdOut),
            GreaterGreaterBar => Ok(Pipe),
            LessLessLess => Ok(String),
            _ => Err(TryFromOperatorError {}),
        }
    }
}

impl From<RedirOp> for Operator {
    fn from(op: RedirOp) -> Operator {
        use Operator::*;
        use RedirOp::*;
        match op {
            FileIn => Less,
            FileInOut => LessGreater,
            FileOut => Greater,
            FileAppend => GreaterGreater,
            FileClobber => GreaterBar,
            FdIn => LessAnd,
            FdOut => GreaterAnd,
            Pipe => GreaterGreaterBar,
            String => LessLessLess,
        }
    }
}

/// Here-document
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HereDoc {
    /// Token that marks the end of the content of the here-document
    pub delimiter: Word,

    /// Whether leading tab characters should be removed from each line of the
    /// here-document content
    ///
    /// This value is `true` for the `<<-` operator and `false` for `<<`.
    pub remove_tabs: bool,

    /// Content of the here-document
    ///
    /// The content ends with a newline unless it is empty. If the delimiter is
    /// quoted, the content must be all literal. If `remove_tabs` is `true`,
    /// each content line does not start with tabs as they are removed when
    /// parsed.
    ///
    /// This value is wrapped in `OnceCell` because the here-doc content is
    /// parsed separately from the here-doc operator. When the operator is
    /// parsed, the `HereDoc` instance is created with an empty content. The
    /// content is filled to the cell when it is parsed later. When accessing
    /// the parsed content, you can safely unwrap the cell.
    pub content: OnceCell<Text>,
}

/// Part of a redirection that defines the nature of the resulting file descriptor
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RedirBody {
    /// Normal redirection
    Normal { operator: RedirOp, operand: Word },
    /// Here-document
    HereDoc(Rc<HereDoc>),
    // TODO process redirection
}

impl RedirBody {
    /// Returns the operand word of the redirection.
    pub fn operand(&self) -> &Word {
        match self {
            RedirBody::Normal { operand, .. } => operand,
            RedirBody::HereDoc(here_doc) => &here_doc.delimiter,
        }
    }
}

impl<T: Into<Rc<HereDoc>>> From<T> for RedirBody {
    fn from(t: T) -> Self {
        RedirBody::HereDoc(t.into())
    }
}

/// Redirection
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Redir {
    /// File descriptor that is modified by this redirection
    pub fd: Option<Fd>,
    /// Nature of the resulting file descriptor
    pub body: RedirBody,
}

impl Redir {
    /// Computes the file descriptor that is modified by this redirection.
    ///
    /// If `self.fd` is `Some(_)`, the `RawFd` value is returned intact. Otherwise,
    /// the default file descriptor is selected depending on the type of `self.body`.
    pub fn fd_or_default(&self) -> Fd {
        use RedirOp::*;
        self.fd.unwrap_or(match self.body {
            RedirBody::Normal { operator, .. } => match operator {
                FileIn | FileInOut | FdIn | String => Fd::STDIN,
                FileOut | FileAppend | FileClobber | FdOut | Pipe => Fd::STDOUT,
            },
            RedirBody::HereDoc { .. } => Fd::STDIN,
        })
    }
}

/// Command that involves assignments, redirections, and word expansions
///
/// In the shell language syntax, a valid simple command must contain at least one of assignments,
/// redirections, and words. The parser must not produce a completely empty simple command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimpleCommand {
    pub assigns: Vec<Assign>,
    pub words: Vec<Word>,
    pub redirs: Rc<Vec<Redir>>,
}

impl SimpleCommand {
    /// Returns true if the simple command does not contain any assignments,
    /// words, or redirections.
    pub fn is_empty(&self) -> bool {
        self.assigns.is_empty() && self.words.is_empty() && self.redirs.is_empty()
    }

    /// Returns true if the simple command contains only one word.
    pub fn is_one_word(&self) -> bool {
        self.assigns.is_empty() && self.words.len() == 1 && self.redirs.is_empty()
    }

    /// Tests whether the first word of the simple command is a keyword.
    #[must_use]
    fn first_word_is_keyword(&self) -> bool {
        let Some(word) = self.words.first() else {
            return false;
        };
        let Some(literal) = word.to_string_if_literal() else {
            return false;
        };
        literal.parse::<Keyword>().is_ok()
    }
}

/// `elif-then` clause
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElifThen {
    pub condition: List,
    pub body: List,
}

/// Symbol that terminates the body of a case branch and determines what to do
/// after executing it
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaseContinuation {
    /// `;;` (terminate the case construct)
    #[default]
    Break,
    /// `;&` (unconditionally execute the body of the next case branch)
    FallThrough,
    /// `;|` or `;;&` (resume with the next case branch, performing pattern matching again)
    Continue,
}

impl TryFrom<Operator> for CaseContinuation {
    type Error = TryFromOperatorError;

    /// Converts an operator into a case continuation.
    ///
    /// The `SemicolonBar` and `SemicolonSemicolonAnd` operators are converted
    /// into `Continue`; you cannot distinguish between the two from the return
    /// value.
    fn try_from(op: Operator) -> Result<CaseContinuation, TryFromOperatorError> {
        use CaseContinuation::*;
        use Operator::*;
        match op {
            SemicolonSemicolon => Ok(Break),
            SemicolonAnd => Ok(FallThrough),
            SemicolonBar | SemicolonSemicolonAnd => Ok(Continue),
            _ => Err(TryFromOperatorError {}),
        }
    }
}

impl From<CaseContinuation> for Operator {
    /// Converts a case continuation into an operator.
    ///
    /// The `Continue` variant is converted into `SemicolonBar`.
    fn from(cc: CaseContinuation) -> Operator {
        use CaseContinuation::*;
        use Operator::*;
        match cc {
            Break => SemicolonSemicolon,
            FallThrough => SemicolonAnd,
            Continue => SemicolonBar,
        }
    }
}

/// Branch item of a `case` compound command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseItem {
    /// Array of patterns that are matched against the main word of the case
    /// compound command to decide if the body of this item should be executed
    ///
    /// A syntactically valid case item must have at least one pattern.
    pub patterns: Vec<Word>,
    /// Commands that are executed if any of the patterns matched
    pub body: List,
    /// What to do after executing the body of this item
    pub continuation: CaseContinuation,
}

/// Command that contains other commands
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompoundCommand {
    /// List as a command
    Grouping(List),
    /// Command for executing commands in a subshell
    Subshell { body: Rc<List>, location: Location },
    /// For loop
    For {
        name: Word,
        values: Option<Vec<Word>>,
        body: List,
    },
    /// While loop
    While { condition: List, body: List },
    /// Until loop
    Until { condition: List, body: List },
    /// If conditional construct
    If {
        condition: List,
        body: List,
        elifs: Vec<ElifThen>,
        r#else: Option<List>,
    },
    /// Case conditional construct
    Case { subject: Word, items: Vec<CaseItem> },
    // TODO [[ ]]
}

/// Compound command with redirections
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullCompoundCommand {
    /// The main part
    pub command: CompoundCommand,
    /// Redirections
    pub redirs: Vec<Redir>,
}

/// Function definition command
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionDefinition {
    /// Whether the function definition command starts with the `function` reserved word
    pub has_keyword: bool,
    /// Function name
    pub name: Word,
    /// Function body
    pub body: Rc<FullCompoundCommand>,
}

/// Element of a pipe sequence
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Simple command
    Simple(SimpleCommand),
    /// Compound command
    Compound(FullCompoundCommand),
    /// Function definition command
    Function(FunctionDefinition),
}

/// Commands separated by `|`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pipeline {
    /// Elements of the pipeline
    ///
    /// A valid pipeline must have at least one command.
    ///
    /// The commands are contained in `Rc` to allow executing them
    /// asynchronously without cloning them.
    pub commands: Vec<Rc<Command>>,
    /// Whether the pipeline begins with a `!`
    pub negation: bool,
}

/// Condition that decides if a [Pipeline] in an [and-or list](AndOrList) should be executed
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndOr {
    /// `&&`
    AndThen,
    /// `||`
    OrElse,
}

impl TryFrom<Operator> for AndOr {
    type Error = TryFromOperatorError;
    fn try_from(op: Operator) -> Result<AndOr, TryFromOperatorError> {
        match op {
            Operator::AndAnd => Ok(AndOr::AndThen),
            Operator::BarBar => Ok(AndOr::OrElse),
            _ => Err(TryFromOperatorError {}),
        }
    }
}

impl From<AndOr> for Operator {
    fn from(op: AndOr) -> Operator {
        match op {
            AndOr::AndThen => Operator::AndAnd,
            AndOr::OrElse => Operator::BarBar,
        }
    }
}

/// Pipelines separated by `&&` and `||`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndOrList {
    pub first: Pipeline,
    pub rest: Vec<(AndOr, Pipeline)>,
}

/// Element of a [List]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    /// Main part of this item
    ///
    /// The and-or list is contained in `Rc` to allow executing it
    /// asynchronously without cloning it.
    pub and_or: Rc<AndOrList>,
    /// Location of the `&` operator for this item, if any
    pub async_flag: Option<Location>,
}

/// Sequence of [and-or lists](AndOrList) separated by `;` or `&`
///
/// It depends on context whether an empty list is a valid syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct List(pub Vec<Item>);

/// Implementations of [std::fmt::Display] for the shell language syntax types
mod impl_display;

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn special_param_from_str() {
        assert_eq!("@".parse(), Ok(SpecialParam::At));
        assert_eq!("*".parse(), Ok(SpecialParam::Asterisk));
        assert_eq!("#".parse(), Ok(SpecialParam::Number));
        assert_eq!("?".parse(), Ok(SpecialParam::Question));
        assert_eq!("-".parse(), Ok(SpecialParam::Hyphen));
        assert_eq!("$".parse(), Ok(SpecialParam::Dollar));
        assert_eq!("!".parse(), Ok(SpecialParam::Exclamation));
        assert_eq!("0".parse(), Ok(SpecialParam::Zero));

        assert_eq!(SpecialParam::from_str(""), Err(NotSpecialParam));
        assert_eq!(SpecialParam::from_str("##"), Err(NotSpecialParam));
        assert_eq!(SpecialParam::from_str("1"), Err(NotSpecialParam));
        assert_eq!(SpecialParam::from_str("00"), Err(NotSpecialParam));
    }

    #[test]
    fn switch_unquote() {
        let switch = Switch {
            r#type: SwitchType::Default,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "foo bar".parse().unwrap(),
        };
        let (unquoted, is_quoted) = switch.unquote();
        assert_eq!(unquoted, ":-foo bar");
        assert_eq!(is_quoted, false);

        let switch = Switch {
            r#type: SwitchType::Error,
            condition: SwitchCondition::Unset,
            word: r"e\r\ror".parse().unwrap(),
        };
        let (unquoted, is_quoted) = switch.unquote();
        assert_eq!(unquoted, "?error");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn trim_unquote() {
        let trim = Trim {
            side: TrimSide::Prefix,
            length: TrimLength::Shortest,
            pattern: "".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "#");
        assert_eq!(is_quoted, false);

        let trim = Trim {
            side: TrimSide::Prefix,
            length: TrimLength::Longest,
            pattern: "'yes'".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "##yes");
        assert_eq!(is_quoted, true);

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Shortest,
            pattern: r"\no".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "%no");
        assert_eq!(is_quoted, true);

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Longest,
            pattern: "?".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "%%?");
        assert_eq!(is_quoted, false);
    }

    #[test]
    fn braced_param_unquote() {
        let param = BracedParam {
            param: Param::variable("foo"),
            modifier: Modifier::None,
            location: Location::dummy(""),
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo}");
        assert_eq!(is_quoted, false);

        let param = BracedParam {
            modifier: Modifier::Length,
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${#foo}");
        assert_eq!(is_quoted, false);

        let switch = Switch {
            r#type: SwitchType::Assign,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "'bar'".parse().unwrap(),
        };
        let param = BracedParam {
            modifier: Modifier::Switch(switch),
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo:=bar}");
        assert_eq!(is_quoted, true);

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Shortest,
            pattern: "baz' 'bar".parse().unwrap(),
        };
        let param = BracedParam {
            modifier: Modifier::Trim(trim),
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo%baz bar}");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn backquote_unit_unquote() {
        let literal = BackquoteUnit::Literal('A');
        let (unquoted, is_quoted) = literal.unquote();
        assert_eq!(unquoted, "A");
        assert_eq!(is_quoted, false);

        let backslashed = BackquoteUnit::Backslashed('X');
        let (unquoted, is_quoted) = backslashed.unquote();
        assert_eq!(unquoted, "X");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn text_from_literal_chars() {
        let text = Text::from_literal_chars(['a', '1'].iter().copied());
        assert_eq!(text.0, [Literal('a'), Literal('1')]);
    }

    #[test]
    fn text_unquote_without_quotes() {
        let empty = Text(vec![]);
        let (unquoted, is_quoted) = empty.unquote();
        assert_eq!(unquoted, "");
        assert_eq!(is_quoted, false);

        let nonempty = Text(vec![
            Literal('W'),
            RawParam {
                param: Param::variable("X"),
                location: Location::dummy(""),
            },
            CommandSubst {
                content: "Y".into(),
                location: Location::dummy(""),
            },
            Backquote {
                content: vec![BackquoteUnit::Literal('Z')],
                location: Location::dummy(""),
            },
            Arith {
                content: Text(vec![Literal('0')]),
                location: Location::dummy(""),
            },
        ]);
        let (unquoted, is_quoted) = nonempty.unquote();
        assert_eq!(unquoted, "W$X$(Y)`Z`$((0))");
        assert_eq!(is_quoted, false);
    }

    #[test]
    fn text_unquote_with_quotes() {
        let quoted = Text(vec![
            Literal('a'),
            Backslashed('b'),
            Literal('c'),
            Arith {
                content: Text(vec![Literal('d')]),
                location: Location::dummy(""),
            },
            Literal('e'),
        ]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "abc$((d))e");
        assert_eq!(is_quoted, true);

        let content = vec![BackquoteUnit::Backslashed('X')];
        let location = Location::dummy("");
        let quoted = Text(vec![Backquote { content, location }]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "`X`");
        assert_eq!(is_quoted, true);

        let content = Text(vec![Backslashed('X')]);
        let location = Location::dummy("");
        let quoted = Text(vec![Arith { content, location }]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "$((X))");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn text_to_string_if_literal_success() {
        let empty = Text(vec![]);
        let s = empty.to_string_if_literal().unwrap();
        assert_eq!(s, "");

        let nonempty = Text(vec![Literal('f'), Literal('o'), Literal('o')]);
        let s = nonempty.to_string_if_literal().unwrap();
        assert_eq!(s, "foo");
    }

    #[test]
    fn text_to_string_if_literal_failure() {
        let backslashed = Text(vec![Backslashed('a')]);
        assert_eq!(backslashed.to_string_if_literal(), None);
    }

    #[test]
    fn word_unquote() {
        let mut word = Word::from_str(r#"~a/b\c'd'"e""#).unwrap();
        let (unquoted, is_quoted) = word.unquote();
        assert_eq!(unquoted, "~a/bcde");
        assert_eq!(is_quoted, true);

        word.parse_tilde_front();
        let (unquoted, is_quoted) = word.unquote();
        assert_eq!(unquoted, "~a/bcde");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn word_to_string_if_literal_success() {
        let empty = Word::from_str("").unwrap();
        let s = empty.to_string_if_literal().unwrap();
        assert_eq!(s, "");

        let nonempty = Word::from_str("~foo").unwrap();
        let s = nonempty.to_string_if_literal().unwrap();
        assert_eq!(s, "~foo");
    }

    #[test]
    fn word_to_string_if_literal_failure() {
        let location = Location::dummy("foo");
        let backslashed = Unquoted(Backslashed('?'));
        let word = Word {
            units: vec![backslashed],
            location,
        };
        assert_eq!(word.to_string_if_literal(), None);

        let word = Word {
            units: vec![Tilde("foo".to_string())],
            ..word
        };
        assert_eq!(word.to_string_if_literal(), None);
    }

    #[test]
    fn assign_try_from_word_without_equal() {
        let word = Word::from_str("foo").unwrap();
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_empty_name() {
        let word = Word::from_str("=foo").unwrap();
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_non_literal_name() {
        let mut word = Word::from_str("night=foo").unwrap();
        word.units.insert(0, Unquoted(Backslashed('k')));
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_literal_name() {
        let word = Word::from_str("night=foo").unwrap();
        let location = word.location.clone();
        let assign = Assign::try_from(word).unwrap();
        assert_eq!(assign.name, "night");
        assert_matches!(assign.value, Scalar(value) => {
            assert_eq!(value.to_string(), "foo");
            assert_eq!(value.location, location);
        });
        assert_eq!(assign.location, location);
    }

    #[test]
    fn assign_try_from_word_tilde() {
        let word = Word::from_str("a=~:~b").unwrap();
        let assign = Assign::try_from(word).unwrap();
        assert_matches!(assign.value, Scalar(value) => {
            assert_eq!(
                value.units,
                [
                    WordUnit::Tilde("".to_string()),
                    WordUnit::Unquoted(TextUnit::Literal(':')),
                    WordUnit::Tilde("b".to_string()),
                ]
            );
        });
    }

    #[test]
    fn redir_op_conversions() {
        use RedirOp::*;
        for op in &[
            FileIn,
            FileInOut,
            FileOut,
            FileAppend,
            FileClobber,
            FdIn,
            FdOut,
            Pipe,
            String,
        ] {
            let op2 = RedirOp::try_from(Operator::from(*op));
            assert_eq!(op2, Ok(*op));
        }
    }

    #[test]
    fn case_continuation_conversions() {
        use CaseContinuation::*;
        for cc in &[Break, FallThrough, Continue] {
            let cc2 = CaseContinuation::try_from(Operator::from(*cc));
            assert_eq!(cc2, Ok(*cc));
        }
        assert_eq!(
            CaseContinuation::try_from(Operator::SemicolonSemicolonAnd),
            Ok(Continue)
        );
    }

    #[test]
    fn and_or_conversions() {
        for op in &[AndOr::AndThen, AndOr::OrElse] {
            let op2 = AndOr::try_from(Operator::from(*op));
            assert_eq!(op2, Ok(*op));
        }
    }
}
