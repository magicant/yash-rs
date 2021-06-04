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

//! Shell command language syntax.
//!
//! This module contains types that represent abstract syntax trees (ASTs) of
//! the shell language.
//!
//! Some types in this module has the type parameter `<H = HereDoc>`. As a user
//! of the AST, you will never have to specify the parameter other than the
//! default `HereDoc`. The parameter with non-default types is used internally
//! by the parser to create intermediate ASTs that lack sub-trees for
//! here-documents, because the contents of here-documents have to be parsed
//! separately from the normal flow of source code.
//!
//! ## Syntactic elements
//!
//! The AST type that represents the whole shell script is [`List`], which is a
//! vector of [`Item`]s. An `Item` is a possibly asynchronous [`AndOrList`],
//! which is a sequence of conditionally executed [`Pipeline`]s. A `Pipeline` is
//! a sequence of [`Command`]s separated by `|`.
//!
//! The shell syntax has many types of `Command`s, that is, [`SimpleCommand`],
//! [`CompoundCommand`] and [`FunctionDefinition`], where `CompoundCommand` in
//! turn has different types.
//!
//! ## Lexical elements
//!
//! Tokens that make up commands may contain quotations and expansions. A
//! [`Word`], a sequence of [`WordUnit`]s, represents such a token that appears
//! in a simple command or some kinds of other commands.
//!
//! In some contexts, tilde expansion and single- and double-quotes are not
//! recognized while other kinds of expansions are allowed. Such part is
//! represented as [`Text`], a sequence of [`TextUnit`]s.
//!
//! ## Parsing
//!
//! Most AST types defined in this modules implement the
//! [`FromStr`](std::str::FromStr) trait, which means you can call `parse` on a
//! `&str` and easily get the parsed AST instance. However, ASTs constructed in
//! this way do not contain very meaningful [source](crate::source) information.
//! All [location](crate::source::Location)s in such ASTs only have [unknown
//! source](crate::source::Source::Unknown).
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
//! assert_eq!(word.location.line.source, Source::Unknown);
//! ```
//!
//! To include a proper source information, you need to prepare a
//! [lexer](crate::parser::lex::Lexer) with source information and then use it
//! to parse the source code. See the [`parser`](crate::parser) module for
//! details.
//!
//! ## Displaying
//!
//! Most AST types support the [`Display`](std::fmt::Display) trait, which
//! allows you to convert an AST to a source code string. Note that the
//! `Display` trait implementations always produce a single-line source code
//! with here-document contents omitted. To pretty-format an AST in multiple
//! lines with here-document contents included, you can use ... TODO TBD.

use crate::parser::lex::Operator;
use crate::source::Location;
use itertools::Itertools;
use std::convert::TryFrom;
use std::fmt;
use std::fmt::Write;
use std::os::unix::io::RawFd;

/// Result of [`Unquote::write_unquoted`].
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

/// Possibly literal syntax element.
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

/// Flag that specifies how the value is substituted in a [switch](Switch).
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

impl fmt::Display for SwitchType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SwitchType::*;
        let c = match self {
            Alter => '+',
            Default => '-',
            Assign => '=',
            Error => '?',
        };
        f.write_char(c)
    }
}

/// Condition that triggers a [switch](Switch).
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

impl fmt::Display for SwitchCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SwitchCondition::*;
        match self {
            Unset => Ok(()),
            UnsetOrEmpty => f.write_char(':'),
        }
    }
}

/// Parameter expansion [modifier](Modifier) that conditionally substitutes the
/// value being expanded.
///
/// Examples of switches include `+foo`, `:-bar` and `:=baz`.
///
/// A switch is composed of a [condition](SwitchCondition) (an optional `:`), a
/// [type](SwitchType) (one of `+`, `-`, `=` and `?`) and a [word](Word).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Switch {
    /// How the value is substituted.
    pub r#type: SwitchType,
    /// Condition that determines whether the value is substituted or not.
    pub condition: SwitchCondition,
    /// Word that substitutes the parameter value.
    pub word: Word,
}

impl fmt::Display for Switch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.condition, self.r#type, self.word)
    }
}

impl Unquote for Switch {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        write!(w, "{}{}", self.condition, self.r#type)?;
        self.word.write_unquoted(w)
    }
}

/// Attribute that modifies a parameter expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Modifier {
    /// No modifier.
    None,
    /// `#` prefix. (`${#foo}`)
    Length,
    /// `+`, `-`, `=` or `?` suffix, optionally with `:`. (`${foo:-bar}`)
    Switch(Switch),
    // TODO Trim
    // TODO Subst
}

/// Parameter expansion enclosed in braces.
///
/// This struct is used only for parameter expansions that are enclosed braces.
/// Expansions that are not enclosed in braces are directly encoded with
/// [`TextUnit::RawParam`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Param {
    // TODO recursive expansion
    /// Parameter name.
    pub name: String,
    // TODO index
    /// Modifier.
    pub modifier: Modifier,
    /// Location of the initial `$` character of this parameter expansion.
    pub location: Location,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Modifier::*;
        match self.modifier {
            None => write!(f, "${{{}}}", self.name),
            Length => write!(f, "${{#{}}}", self.name),
            Switch(ref switch) => write!(f, "${{{}{}}}", self.name, switch),
        }
    }
}

impl Unquote for Param {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        use Modifier::*;
        match self.modifier {
            None => {
                write!(w, "${{{}}}", self.name)?;
                Ok(false)
            }
            Length => {
                write!(w, "${{#{}}}", self.name)?;
                Ok(false)
            }
            Switch(ref switch) => {
                write!(w, "${{{}", self.name)?;
                let quoted = switch.write_unquoted(w)?;
                w.write_char('}')?;
                Ok(quoted)
            }
        }
    }
}

/// Element of [`TextUnit::Backquote`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackquoteUnit {
    /// Literal single character.
    Literal(char),
    /// Backslash-escaped single character.
    Backslashed(char),
}

impl fmt::Display for BackquoteUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackquoteUnit::Literal(c) => write!(f, "{}", c),
            BackquoteUnit::Backslashed(c) => write!(f, "\\{}", c),
        }
    }
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

/// Element of a [Text], i.e., something that can be expanded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextUnit {
    /// Literal single character.
    Literal(char),
    /// Backslash-escaped single character.
    Backslashed(char),
    /// Parameter expansion that is not enclosed in braces.
    RawParam {
        /// Parameter name.
        name: String,
        /// Location of the initial `$` character of this parameter expansion.
        location: Location,
    },
    /// Parameter expansion that is enclosed in braces.
    BracedParam(Param),
    /// Command substitution of the form `$(...)`.
    CommandSubst {
        /// Command string that will be parsed and executed when the command
        /// substitution is expanded.
        content: String,
        /// Location of the initial `$` character of this command substitution.
        location: Location,
    },
    /// Command substitution of the form `` `...` ``.
    Backquote {
        /// Command string that will be parsed and executed when the command
        /// substitution is expanded.
        content: Vec<BackquoteUnit>,
        /// Location of the initial backquote character of this command substitution.
        location: Location,
    },
    /// Arithmetic expansion.
    Arith {
        /// Expression that is to be evaluated.
        content: Text,
        /// Location of the initial `$` character of this command substitution.
        location: Location,
    },
}

pub use TextUnit::*;

impl fmt::Display for TextUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal(c) => write!(f, "{}", c),
            Backslashed(c) => write!(f, "\\{}", c),
            RawParam { name, .. } => write!(f, "${}", name),
            BracedParam(param) => param.fmt(f),
            CommandSubst { content, .. } => write!(f, "$({})", content),
            Backquote { content, .. } => {
                f.write_char('`')?;
                content.iter().try_for_each(|unit| unit.fmt(f))?;
                f.write_char('`')
            }
            Arith { content, .. } => write!(f, "$(({}))", content),
        }
    }
}

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
            RawParam { name, .. } => {
                write!(w, "${}", name)?;
                Ok(false)
            }
            BracedParam(param) => param.write_unquoted(w),
            CommandSubst { content, .. } => {
                write!(w, "$({})", content)?;
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

/// String that may contain some expansions.
///
/// A text is a sequence of [text unit](TextUnit)s, which may contain some kinds
/// of expansions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Text(pub Vec<TextUnit>);

impl Text {
    /// Creates a text from an iterator of literal chars.
    pub fn from_literal_chars<I: IntoIterator<Item = char>>(i: I) -> Text {
        Text(i.into_iter().map(Literal).collect())
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|unit| unit.fmt(f))
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

/// Element of a [Word], i.e., text with quotes and tilde expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WordUnit {
    /// Unquoted [`TextUnit`] as a word unit.
    Unquoted(TextUnit),
    /// String surrounded with a pair of single quotations.
    SingleQuote(String),
    /// Text surrounded with a pair of double quotations.
    DoubleQuote(Text),
    /// Tilde expansion.
    ///
    /// The `String` value does not contain the initial tilde.
    Tilde(String),
}

pub use WordUnit::*;

impl fmt::Display for WordUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unquoted(dq) => dq.fmt(f),
            SingleQuote(s) => write!(f, "'{}'", s),
            DoubleQuote(content) => write!(f, "\"{}\"", content),
            Tilde(s) => write!(f, "~{}", s),
        }
    }
}

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
                write!(w, "~{}", s)?;
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

/// Token that may involve expansions and quotes.
///
/// A word is a sequence of [word unit](WordUnit)s. It depends on context whether
/// an empty word is valid or not. It is your responsibility to ensure a word is
/// non-empty in a context where it cannot.
///
/// The difference between words and [text](Text)s is that only words can contain
/// single- and double-quotes and tilde expansions. Compare [`WordUnit`] and [`TextUnit`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Word {
    /// Word units that constitute the word.
    pub units: Vec<WordUnit>,
    /// Location of the first character of the word.
    pub location: Location,
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.units.iter().try_for_each(|unit| write!(f, "{}", unit))
    }
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

/// Value of an [assignment](Assign).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Scalar value, a possibly empty word.
    ///
    /// Note: Because a scalar assignment value is created from a normal command
    /// word, the location of the word in the scalar value points to the first
    /// character of the entire assignment word rather than that of the assigned
    /// value.
    Scalar(Word),

    /// Array, possibly empty list of non-empty words.
    ///
    /// Array assignment is a POSIXly non-portable extension.
    Array(Vec<Word>),
}

pub use Value::*;

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scalar(word) => word.fmt(f),
            Array(words) => write!(f, "({})", words.iter().format(" ")),
        }
    }
}

/// Assignment word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assign {
    /// Name of the variable to assign to.
    ///
    /// In the valid assignment syntax, the name must not be empty.
    pub name: String,
    /// Value assigned to the variable.
    pub value: Value,
    /// Location of the first character of the assignment word.
    pub location: Location,
}

impl fmt::Display for Assign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", &self.name, &self.value)
    }
}

/// Fallible conversion from a word into an assignment.
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

/// Redirection operators.
///
/// This enum defines the redirection operator types except here-document and
/// process redirection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedirOp {
    // `<` (open a file for input)
    FileIn,
    // `<>` (open a file for input and output)
    FileInOut,
    // `>` (open a file for output; truncate or fail if existing)
    FileOut,
    // `>>` (open a file for output; append if existing)
    FileAppend,
    // `>|` (open a file for output; always truncate if existing)
    FileClobber,
    // `<&` (copy or close a file descriptor for input)
    FdIn,
    // `>&` (copy or close a file descriptor for output)
    FdOut,
    // `>>|` (open a pipe, one end for input and the other output)
    Pipe,
    // `<<<` (here-string)
    String,
}

impl TryFrom<Operator> for RedirOp {
    type Error = ();
    fn try_from(op: Operator) -> Result<RedirOp, ()> {
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
            _ => Err(()),
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

impl fmt::Display for RedirOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Operator::from(*self).fmt(f)
    }
}

/// Here-document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HereDoc {
    /// Token that marks the end of the content of the here-document.
    pub delimiter: Word,

    /// Whether leading tab characters should be removed from each line of the
    /// here-document content. This value is `true` for the `<<-` operator and
    /// `false` for `<<`.
    pub remove_tabs: bool,

    /// Content of the here-document.
    ///
    /// The content ends with a newline unless it is empty. If the delimiter
    /// is quoted, the content must be all literal.
    pub content: Text,
}

impl fmt::Display for HereDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.remove_tabs { "<<-" } else { "<<" })?;

        // This space is to disambiguate `<< --` and `<<- -`
        if let Some(Unquoted(Literal('-'))) = self.delimiter.units.get(0) {
            f.write_char(' ')?;
        }

        write!(f, "{}", self.delimiter)
    }
}

/// Part of a redirection that defines the nature of the resulting file descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RedirBody<H = HereDoc> {
    /// Normal redirection.
    Normal { operator: RedirOp, operand: Word },
    /// Here-document.
    HereDoc(H),
    // TODO process redirection
}

impl<H: fmt::Display> fmt::Display for RedirBody<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirBody::Normal { operator, operand } => write!(f, "{}{}", operator, operand),
            RedirBody::HereDoc(h) => write!(f, "{}", h),
        }
    }
}

impl From<HereDoc> for RedirBody {
    fn from(h: HereDoc) -> Self {
        RedirBody::HereDoc(h)
    }
}

/// Redirection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Redir<H = HereDoc> {
    /// File descriptor that is modified by this redirection.
    pub fd: Option<RawFd>,
    /// Nature of the resulting file descriptor.
    pub body: RedirBody<H>,
}

// TODO Should be somewhere else.
const STDIN_FD: RawFd = 0;
const STDOUT_FD: RawFd = 1;

impl<H> Redir<H> {
    /// Computes the file descriptor that is modified by this redirection.
    ///
    /// If `self.fd` is `Some(_)`, the `RawFd` value is returned intact. Otherwise,
    /// the default file descriptor is selected depending on the type of `self.body`.
    pub fn fd_or_default(&self) -> RawFd {
        use RedirOp::*;
        self.fd.unwrap_or_else(|| match self.body {
            RedirBody::Normal { operator, .. } => match operator {
                FileIn | FileInOut | FdIn | String => STDIN_FD,
                FileOut | FileAppend | FileClobber | FdOut | Pipe => STDOUT_FD,
            },
            RedirBody::HereDoc { .. } => STDIN_FD,
        })
    }
}

impl<H: fmt::Display> fmt::Display for Redir<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(fd) = self.fd {
            write!(f, "{}", fd)?;
        }
        write!(f, "{}", self.body)
    }
}

/// Command that involves assignments, redirections, and word expansions.
///
/// In the shell language syntax, a valid simple command must contain at least one of assignments,
/// redirections, and words. The parser must not produce a completely empty simple command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimpleCommand<H = HereDoc> {
    pub assigns: Vec<Assign>,
    pub words: Vec<Word>,
    pub redirs: Vec<Redir<H>>,
}

impl<H> SimpleCommand<H> {
    /// Returns true if the simple command does not contain any assignments,
    /// words, or redirections.
    pub fn is_empty(&self) -> bool {
        self.assigns.is_empty() && self.words.is_empty() && self.redirs.is_empty()
    }

    /// Returns true if the simple command contains only one word.
    pub fn is_one_word(&self) -> bool {
        self.assigns.is_empty() && self.words.len() == 1 && self.redirs.is_empty()
    }
}

impl<H: fmt::Display> fmt::Display for SimpleCommand<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i1 = self.assigns.iter().map(|x| x as &dyn fmt::Display);
        let i2 = self.words.iter().map(|x| x as &dyn fmt::Display);
        let i3 = self.redirs.iter().map(|x| x as &dyn fmt::Display);
        write!(f, "{}", i1.chain(i2).chain(i3).format(" "))
        // TODO Avoid printing a keyword as the first word
    }
}

/// `elif-then` clause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElifThen<H = HereDoc> {
    pub condition: List<H>,
    pub body: List<H>,
}

impl<H: fmt::Display> fmt::Display for ElifThen<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elif {:#} then ", self.condition)?;
        if f.alternate() {
            write!(f, "{:#}", self.body)
        } else {
            write!(f, "{}", self.body)
        }
    }
}

/// Branch item of a `case` compound command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseItem<H = HereDoc> {
    /// Array of patterns that are matched against the main word of the case
    /// compound command to decide if the body of this item should be executed.
    ///
    /// A syntactically valid case item must have at least one pattern.
    pub patterns: Vec<Word>,
    /// Commands that are executed if any of the patterns matched.
    pub body: List<H>,
}

impl<H: fmt::Display> fmt::Display for CaseItem<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({}) {};;",
            self.patterns.iter().format(" | "),
            self.body
        )
    }
}

/// Command that contains other commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompoundCommand<H = HereDoc> {
    /// List as a command.
    Grouping(List<H>),
    /// Command for executing commands in a subshell.
    Subshell(List<H>),
    /// For loop.
    For {
        name: Word,
        values: Option<Vec<Word>>,
        body: List<H>,
    },
    /// While loop.
    While { condition: List<H>, body: List<H> },
    /// Until loop.
    Until { condition: List<H>, body: List<H> },
    /// If conditional construct.
    If {
        condition: List<H>,
        body: List<H>,
        elifs: Vec<ElifThen<H>>,
        r#else: Option<List<H>>,
    },
    /// Case conditional construct.
    Case {
        subject: Word,
        items: Vec<CaseItem<H>>,
    },
    // TODO [[ ]]
}

impl<H: fmt::Display> fmt::Display for CompoundCommand<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CompoundCommand::*;
        match self {
            Grouping(list) => write!(f, "{{ {:#} }}", list),
            Subshell(list) => write!(f, "({})", list),
            For { name, values, body } => {
                write!(f, "for {}", name)?;
                if let Some(values) = values {
                    f.write_str(" in")?;
                    for value in values {
                        write!(f, " {}", value)?;
                    }
                    f.write_char(';')?;
                }
                write!(f, " do {:#} done", body)
            }
            While { condition, body } => write!(f, "while {:#} do {:#} done", condition, body),
            Until { condition, body } => write!(f, "until {:#} do {:#} done", condition, body),
            If {
                condition,
                body,
                elifs,
                r#else,
            } => {
                write!(f, "if {:#} then {:#} ", condition, body)?;
                for elif in elifs {
                    write!(f, "{:#} ", elif)?;
                }
                if let Some(r#else) = r#else {
                    write!(f, "else {:#} ", r#else)?;
                }
                f.write_str("fi")
            }
            Case { subject, items } => {
                write!(f, "case {} in ", subject)?;
                for item in items {
                    write!(f, "{} ", item)?;
                }
                f.write_str("esac")
            }
        }
    }
}

/// Compound command with redirections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullCompoundCommand<H = HereDoc> {
    /// The main part.
    pub command: CompoundCommand<H>,
    /// Redirections.
    pub redirs: Vec<Redir<H>>,
}

impl<H: fmt::Display> fmt::Display for FullCompoundCommand<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let FullCompoundCommand { command, redirs } = self;
        write!(f, "{}", command)?;
        redirs.iter().try_for_each(|redir| write!(f, " {}", redir))
    }
}

/// Function definition command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionDefinition<H = HereDoc> {
    /// Whether the function definition command starts with the `function` reserved word.
    pub has_keyword: bool,
    /// Function name.
    pub name: Word,
    /// Function body.
    pub body: FullCompoundCommand<H>,
}

impl<H: fmt::Display> fmt::Display for FunctionDefinition<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.has_keyword {
            f.write_str("function ")?;
        }
        write!(f, "{}() {}", self.name, self.body)
    }
}

/// Element of a pipe sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command<H = HereDoc> {
    /// Simple command.
    Simple(SimpleCommand<H>),
    /// Compound command.
    Compound(FullCompoundCommand<H>),
    /// Function definition command.
    Function(FunctionDefinition<H>),
}

impl<H: fmt::Display> fmt::Display for Command<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Simple(c) => c.fmt(f),
            Command::Compound(c) => c.fmt(f),
            Command::Function(c) => c.fmt(f),
        }
    }
}

/// Commands separated by `|`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pipeline<H = HereDoc> {
    /// Elements of the pipeline.
    ///
    /// A valid pipeline must have at least one command.
    pub commands: Vec<Command<H>>,
    /// True if the pipeline begins with a `!`.
    pub negation: bool,
}

impl<H: fmt::Display> fmt::Display for Pipeline<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        if self.negation {
            write!(f, "! ")?;
        }
        write!(f, "{}", self.commands.iter().format(" | "))
    }
}

/// Condition that decides if a [Pipeline] in an [and-or list](AndOrList) should be executed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndOr {
    /// `&&`
    AndThen,
    /// `||`
    OrElse,
}

impl TryFrom<Operator> for AndOr {
    type Error = ();
    fn try_from(op: Operator) -> Result<AndOr, ()> {
        match op {
            Operator::AndAnd => Ok(AndOr::AndThen),
            Operator::BarBar => Ok(AndOr::OrElse),
            _ => Err(()),
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

impl fmt::Display for AndOr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AndOr::AndThen => write!(f, "&&"),
            AndOr::OrElse => write!(f, "||"),
        }
    }
}

/// Pipelines separated by `&&` and `||`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndOrList<H = HereDoc> {
    pub first: Pipeline<H>,
    pub rest: Vec<(AndOr, Pipeline<H>)>,
}

impl<H: fmt::Display> fmt::Display for AndOrList<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.first)?;
        self.rest
            .iter()
            .try_for_each(|(c, p)| write!(f, " {} {}", c, p))
    }
}

/// Element of a [List].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item<H = HereDoc> {
    /// Main part of this item.
    pub and_or: AndOrList<H>,
    /// True if this item is terminated by `&`.
    pub is_async: bool,
}

/// Allows conversion from Item to String.
///
/// By default, the `;` terminator is omitted from the formatted string.
/// When the alternate flag is specified as in `{:#}`, the result is always
/// terminated by either `;` or `&`.
impl<H: fmt::Display> fmt::Display for Item<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.and_or)?;
        if self.is_async {
            write!(f, "&")
        } else if f.alternate() {
            write!(f, ";")
        } else {
            Ok(())
        }
    }
}

/// Sequence of [and-or lists](AndOrList) separated by `;` or `&`.
///
/// It depends on context whether an empty list is a valid syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct List<H = HereDoc>(pub Vec<Item<H>>);

/// Allows conversion from List to String.
///
/// By default, the last `;` terminator is omitted from the formatted string.
/// When the alternate flag is specified as in `{:#}`, the result is always
/// terminated by either `;` or `&`.
impl<H: fmt::Display> fmt::Display for List<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((last, others)) = self.0.split_last() {
            for item in others {
                write!(f, "{:#} ", item)?;
            }
            if f.alternate() {
                write!(f, "{:#}", last)
            } else {
                write!(f, "{}", last)
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn switch_display() {
        let switch = Switch {
            r#type: SwitchType::Alter,
            condition: SwitchCondition::Unset,
            word: "".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), "+");

        let switch = Switch {
            r#type: SwitchType::Default,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "foo".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), ":-foo");

        let switch = Switch {
            r#type: SwitchType::Assign,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "bar baz".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), ":=bar baz");

        let switch = Switch {
            r#type: SwitchType::Error,
            condition: SwitchCondition::Unset,
            word: "?error".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), "??error");
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
    fn braced_param_display() {
        let param = Param {
            name: "foo".to_string(),
            modifier: Modifier::None,
            location: Location::dummy("".to_string()),
        };
        assert_eq!(param.to_string(), "${foo}");

        let param = Param {
            modifier: Modifier::Length,
            ..param
        };
        assert_eq!(param.to_string(), "${#foo}");

        let switch = Switch {
            r#type: SwitchType::Assign,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "bar baz".parse().unwrap(),
        };
        let param = Param {
            modifier: Modifier::Switch(switch),
            ..param
        };
        assert_eq!(param.to_string(), "${foo:=bar baz}");
    }

    #[test]
    fn braced_param_unquote() {
        let param = Param {
            name: "foo".to_string(),
            modifier: Modifier::None,
            location: Location::dummy("".to_string()),
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo}");
        assert_eq!(is_quoted, false);

        let param = Param {
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
        let param = Param {
            modifier: Modifier::Switch(switch),
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo:=bar}");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn backquote_unit_display() {
        let literal = BackquoteUnit::Literal('A');
        assert_eq!(literal.to_string(), "A");
        let backslashed = BackquoteUnit::Backslashed('X');
        assert_eq!(backslashed.to_string(), r"\X");
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
    fn text_unit_display() {
        let literal = Literal('A');
        assert_eq!(literal.to_string(), "A");
        let backslashed = Backslashed('X');
        assert_eq!(backslashed.to_string(), r"\X");

        let raw_param = RawParam {
            name: "PARAM".to_string(),
            location: Location::dummy("".to_string()),
        };
        assert_eq!(raw_param.to_string(), "$PARAM");

        let command_subst = CommandSubst {
            content: r"foo\bar".to_string(),
            location: Location::dummy("".to_string()),
        };
        assert_eq!(command_subst.to_string(), r"$(foo\bar)");

        let backquote = Backquote {
            content: vec![
                BackquoteUnit::Literal('a'),
                BackquoteUnit::Backslashed('b'),
                BackquoteUnit::Backslashed('c'),
                BackquoteUnit::Literal('d'),
            ],
            location: Location::dummy("".to_string()),
        };
        assert_eq!(backquote.to_string(), r"`a\b\cd`");

        let arith = Arith {
            content: Text(vec![literal, backslashed, command_subst, backquote]),
            location: Location::dummy("".to_string()),
        };
        assert_eq!(arith.to_string(), r"$((A\X$(foo\bar)`a\b\cd`))");
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
                name: "X".to_string(),
                location: Location::dummy("".to_string()),
            },
            CommandSubst {
                content: "Y".to_string(),
                location: Location::dummy("".to_string()),
            },
            Backquote {
                content: vec![BackquoteUnit::Literal('Z')],
                location: Location::dummy("".to_string()),
            },
            Arith {
                content: Text(vec![Literal('0')]),
                location: Location::dummy("".to_string()),
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
                location: Location::dummy("".to_string()),
            },
            Literal('e'),
        ]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "abc$((d))e");
        assert_eq!(is_quoted, true);

        let content = vec![BackquoteUnit::Backslashed('X')];
        let location = Location::dummy("".to_string());
        let quoted = Text(vec![Backquote { content, location }]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "`X`");
        assert_eq!(is_quoted, true);

        let content = Text(vec![Backslashed('X')]);
        let location = Location::dummy("".to_string());
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
    fn word_unit_display() {
        let unquoted = Unquoted(Literal('A'));
        assert_eq!(unquoted.to_string(), "A");
        let unquoted = Unquoted(Backslashed('B'));
        assert_eq!(unquoted.to_string(), "\\B");

        let single_quote = SingleQuote("".to_string());
        assert_eq!(single_quote.to_string(), "''");
        let single_quote = SingleQuote(r#"a"b"c\"#.to_string());
        assert_eq!(single_quote.to_string(), r#"'a"b"c\'"#);

        let double_quote = DoubleQuote(Text(vec![]));
        assert_eq!(double_quote.to_string(), "\"\"");
        let double_quote = DoubleQuote(Text(vec![Literal('A'), Backslashed('B')]));
        assert_eq!(double_quote.to_string(), "\"A\\B\"");

        let tilde = Tilde("".to_string());
        assert_eq!(tilde.to_string(), "~");
        let tilde = Tilde("foo".to_string());
        assert_eq!(tilde.to_string(), "~foo");
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
        let location = Location::dummy("foo".to_string());
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
    fn scalar_display() {
        let s = Scalar(Word::from_str("my scalar value").unwrap());
        assert_eq!(s.to_string(), "my scalar value");
    }

    #[test]
    fn array_display_empty() {
        let a = Array(vec![]);
        assert_eq!(a.to_string(), "()");
    }

    #[test]
    fn array_display_one() {
        let a = Array(vec![Word::from_str("one").unwrap()]);
        assert_eq!(a.to_string(), "(one)");
    }

    #[test]
    fn array_display_many() {
        let a = Array(vec![
            Word::from_str("let").unwrap(),
            Word::from_str("me").unwrap(),
            Word::from_str("see").unwrap(),
        ]);
        assert_eq!(a.to_string(), "(let me see)");
    }

    #[test]
    fn assign_display() {
        let mut a = Assign::from_str("foo=bar").unwrap();
        assert_eq!(a.to_string(), "foo=bar");

        a.value = Array(vec![]);
        assert_eq!(a.to_string(), "foo=()");
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
        if let Scalar(value) = assign.value {
            assert_eq!(value.to_string(), "foo");
            assert_eq!(value.location, location);
        } else {
            panic!("wrong value: {:?}", assign.value);
        }
        assert_eq!(assign.location, location);
    }

    #[test]
    fn assign_try_from_word_tilde() {
        let word = Word::from_str("a=~:~b").unwrap();
        let assign = Assign::try_from(word).unwrap();
        if let Scalar(value) = assign.value {
            assert_eq!(
                value.units,
                [
                    WordUnit::Tilde("".to_string()),
                    WordUnit::Unquoted(TextUnit::Literal(':')),
                    WordUnit::Tilde("b".to_string()),
                ]
            );
        } else {
            panic!("wrong value: {:?}", assign.value);
        }
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
    fn here_doc_display() {
        let heredoc = HereDoc {
            delimiter: Word::from_str("END").unwrap(),
            remove_tabs: true,
            content: Text::from_str("here").unwrap(),
        };
        assert_eq!(heredoc.to_string(), "<<-END");

        let heredoc = HereDoc {
            delimiter: Word::from_str("XXX").unwrap(),
            remove_tabs: false,
            content: Text::from_str("there").unwrap(),
        };
        assert_eq!(heredoc.to_string(), "<<XXX");
    }

    #[test]
    fn here_doc_display_disambiguation() {
        let heredoc = HereDoc {
            delimiter: Word::from_str("--").unwrap(),
            remove_tabs: false,
            content: Text::from_str("here").unwrap(),
        };
        assert_eq!(heredoc.to_string(), "<< --");

        let heredoc = HereDoc {
            delimiter: Word::from_str("-").unwrap(),
            remove_tabs: true,
            content: Text::from_str("here").unwrap(),
        };
        assert_eq!(heredoc.to_string(), "<<- -");
    }

    #[test]
    fn redir_display() {
        let heredoc = HereDoc {
            delimiter: Word::from_str("END").unwrap(),
            remove_tabs: false,
            content: Text::from_str("here").unwrap(),
        };

        let redir = Redir {
            fd: None,
            body: heredoc.into(),
        };
        assert_eq!(redir.to_string(), "<<END");
        let redir = Redir {
            fd: Some(0),
            ..redir
        };
        assert_eq!(redir.to_string(), "0<<END");
        let redir = Redir {
            fd: Some(9),
            ..redir
        };
        assert_eq!(redir.to_string(), "9<<END");
    }

    #[test]
    fn simple_command_display() {
        let mut command = SimpleCommand {
            assigns: vec![],
            words: vec![],
            redirs: vec![],
        };
        assert_eq!(command.to_string(), "");

        command
            .assigns
            .push(Assign::from_str("name=value").unwrap());
        assert_eq!(command.to_string(), "name=value");

        command
            .assigns
            .push(Assign::from_str("hello=world").unwrap());
        assert_eq!(command.to_string(), "name=value hello=world");

        command.words.push(Word::from_str("echo").unwrap());
        assert_eq!(command.to_string(), "name=value hello=world echo");

        command.words.push(Word::from_str("foo").unwrap());
        assert_eq!(command.to_string(), "name=value hello=world echo foo");

        command.redirs.push(Redir {
            fd: None,
            body: RedirBody::from(HereDoc {
                delimiter: Word::from_str("END").unwrap(),
                remove_tabs: false,
                content: Text::from_str("").unwrap(),
            }),
        });
        assert_eq!(command.to_string(), "name=value hello=world echo foo <<END");

        command.assigns.clear();
        assert_eq!(command.to_string(), "echo foo <<END");

        command.words.clear();
        assert_eq!(command.to_string(), "<<END");

        command.redirs.push(Redir {
            fd: Some(1),
            body: RedirBody::from(HereDoc {
                delimiter: Word::from_str("here").unwrap(),
                remove_tabs: true,
                content: Text::from_str("ignored").unwrap(),
            }),
        });
        assert_eq!(command.to_string(), "<<END 1<<-here");

        command.assigns.push(Assign::from_str("foo=bar").unwrap());
        assert_eq!(command.to_string(), "foo=bar <<END 1<<-here");
    }

    #[test]
    fn elif_then_display() {
        let condition: List = "c 1& c 2".parse().unwrap();
        let body = "b 1& b 2".parse().unwrap();
        let elif = ElifThen { condition, body };
        assert_eq!(format!("{}", elif), "elif c 1& c 2; then b 1& b 2");
        assert_eq!(format!("{:#}", elif), "elif c 1& c 2; then b 1& b 2;");

        let condition: List = "c&".parse().unwrap();
        let body = "b&".parse().unwrap();
        let elif = ElifThen { condition, body };
        assert_eq!(format!("{}", elif), "elif c& then b&");
        assert_eq!(format!("{:#}", elif), "elif c& then b&");
    }

    #[test]
    fn case_item_display() {
        let patterns = vec!["foo".parse().unwrap()];
        let body = "".parse::<List>().unwrap();
        let item = CaseItem { patterns, body };
        assert_eq!(item.to_string(), "(foo) ;;");

        let patterns = vec!["bar".parse().unwrap()];
        let body = "echo ok".parse::<List>().unwrap();
        let item = CaseItem { patterns, body };
        assert_eq!(item.to_string(), "(bar) echo ok;;");

        let patterns = ["a", "b", "c"].iter().map(|s| s.parse().unwrap()).collect();
        let body = "foo; bar&".parse::<List>().unwrap();
        let item = CaseItem { patterns, body };
        assert_eq!(item.to_string(), "(a | b | c) foo; bar&;;");
    }

    #[test]
    fn grouping_display() {
        let list = "foo".parse::<List>().unwrap();
        let grouping = CompoundCommand::Grouping(list);
        assert_eq!(grouping.to_string(), "{ foo; }");
    }

    #[test]
    fn for_display_without_values() {
        let name = Word::from_str("foo").unwrap();
        let values = None;
        let body = "echo ok".parse::<List>().unwrap();
        let r#for = CompoundCommand::For { name, values, body };
        assert_eq!(r#for.to_string(), "for foo do echo ok; done");
    }

    #[test]
    fn for_display_with_empty_values() {
        let name = Word::from_str("foo").unwrap();
        let values = Some(vec![]);
        let body = "echo ok".parse::<List>().unwrap();
        let r#for = CompoundCommand::For { name, values, body };
        assert_eq!(r#for.to_string(), "for foo in; do echo ok; done");
    }

    #[test]
    fn for_display_with_some_values() {
        let name = Word::from_str("V").unwrap();
        let values = Some(vec![
            Word::from_str("a").unwrap(),
            Word::from_str("b").unwrap(),
        ]);
        let body = "one; two&".parse::<List>().unwrap();
        let r#for = CompoundCommand::For { name, values, body };
        assert_eq!(r#for.to_string(), "for V in a b; do one; two& done");
    }

    #[test]
    fn while_display() {
        let condition = "true& false".parse::<List>().unwrap();
        let body = "echo ok".parse::<List>().unwrap();
        let r#while = CompoundCommand::While { condition, body };
        assert_eq!(r#while.to_string(), "while true& false; do echo ok; done");
    }

    #[test]
    fn until_display() {
        let condition = "true& false".parse::<List>().unwrap();
        let body = "echo ok".parse::<List>().unwrap();
        let until = CompoundCommand::Until { condition, body };
        assert_eq!(until.to_string(), "until true& false; do echo ok; done");
    }

    #[test]
    fn if_display() {
        let r#if: CompoundCommand = CompoundCommand::If {
            condition: "c 1; c 2&".parse().unwrap(),
            body: "b 1; b 2&".parse().unwrap(),
            elifs: vec![],
            r#else: None,
        };
        assert_eq!(r#if.to_string(), "if c 1; c 2& then b 1; b 2& fi");

        let r#if: CompoundCommand = CompoundCommand::If {
            condition: "c 1& c 2;".parse().unwrap(),
            body: "b 1& b 2;".parse().unwrap(),
            elifs: vec![ElifThen {
                condition: "c 3&".parse().unwrap(),
                body: "b 3&".parse().unwrap(),
            }],
            r#else: Some("b 4".parse().unwrap()),
        };
        assert_eq!(
            r#if.to_string(),
            "if c 1& c 2; then b 1& b 2; elif c 3& then b 3& else b 4; fi"
        );

        let r#if: CompoundCommand = CompoundCommand::If {
            condition: "true".parse().unwrap(),
            body: ":".parse().unwrap(),
            elifs: vec![
                ElifThen {
                    condition: "false".parse().unwrap(),
                    body: "a".parse().unwrap(),
                },
                ElifThen {
                    condition: "echo&".parse().unwrap(),
                    body: "b&".parse().unwrap(),
                },
            ],
            r#else: None,
        };
        assert_eq!(
            r#if.to_string(),
            "if true; then :; elif false; then a; elif echo& then b& fi"
        );
    }

    #[test]
    fn case_display() {
        let subject = "foo".parse().unwrap();
        let items = Vec::<CaseItem>::new();
        let case = CompoundCommand::Case { subject, items };
        assert_eq!(case.to_string(), "case foo in esac");

        let subject = "bar".parse().unwrap();
        let items = vec!["foo)".parse().unwrap()];
        let case = CompoundCommand::Case { subject, items };
        assert_eq!(case.to_string(), "case bar in (foo) ;; esac");

        let subject = "baz".parse().unwrap();
        let items = vec!["1)".parse().unwrap(), "(a|b|c) :&".parse().unwrap()];
        let case = CompoundCommand::Case { subject, items };
        assert_eq!(case.to_string(), "case baz in (1) ;; (a | b | c) :&;; esac");
    }

    #[test]
    fn function_definition_display() {
        let body = FullCompoundCommand {
            command: "( bar )".parse().unwrap(),
            redirs: vec![],
        };
        let fd = FunctionDefinition {
            has_keyword: false,
            name: Word::from_str("foo").unwrap(),
            body,
        };
        assert_eq!(fd.to_string(), "foo() (bar)");
    }

    #[test]
    fn pipeline_display() {
        let mut p = Pipeline {
            commands: vec![],
            negation: false,
        };
        p.commands.push("first".parse().unwrap());
        assert_eq!(p.to_string(), "first");

        p.negation = true;
        assert_eq!(p.to_string(), "! first");

        p.commands.push("second".parse().unwrap());
        assert_eq!(p.to_string(), "! first | second");

        p.commands.push("third".parse().unwrap());
        p.negation = false;
        assert_eq!(p.to_string(), "first | second | third");
    }

    #[test]
    fn and_or_conversions() {
        for op in &[AndOr::AndThen, AndOr::OrElse] {
            let op2 = AndOr::try_from(Operator::from(*op));
            assert_eq!(op2, Ok(*op));
        }
    }

    #[test]
    fn and_or_list_display() {
        let p = "first".parse().unwrap();
        let mut aol = AndOrList {
            first: p,
            rest: vec![],
        };
        assert_eq!(aol.to_string(), "first");

        let p = "second".parse().unwrap();
        aol.rest.push((AndOr::AndThen, p));
        assert_eq!(aol.to_string(), "first && second");

        let p = "third".parse().unwrap();
        aol.rest.push((AndOr::OrElse, p));
        assert_eq!(aol.to_string(), "first && second || third");
    }

    #[test]
    fn list_display() {
        let and_or = "first".parse().unwrap();
        let item = Item {
            and_or,
            is_async: false,
        };
        let mut list = List(vec![item]);
        assert_eq!(list.to_string(), "first");

        let and_or = "second".parse().unwrap();
        let item = Item {
            and_or,
            is_async: true,
        };
        list.0.push(item);
        assert_eq!(list.to_string(), "first; second&");

        let and_or = "third".parse().unwrap();
        let item = Item {
            and_or,
            is_async: false,
        };
        list.0.push(item);
        assert_eq!(list.to_string(), "first; second& third");
    }

    #[test]
    fn list_display_alternate() {
        let and_or = "first".parse().unwrap();
        let item = Item {
            and_or,
            is_async: false,
        };
        let mut list = List(vec![item]);
        assert_eq!(format!("{:#}", list), "first;");

        let and_or = "second".parse().unwrap();
        let item = Item {
            and_or,
            is_async: true,
        };
        list.0.push(item);
        assert_eq!(format!("{:#}", list), "first; second&");

        let and_or = "third".parse().unwrap();
        let item = Item {
            and_or,
            is_async: false,
        };
        list.0.push(item);
        assert_eq!(format!("{:#}", list), "first; second& third;");
    }
}
