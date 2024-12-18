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
#[cfg(unix)]
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::str::FromStr;

#[cfg(not(unix))]
type RawFd = i32;

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

/// Element of [`TextUnit::Backquote`]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackquoteUnit {
    /// Literal single character
    Literal(char),
    /// Backslash-escaped single character
    Backslashed(char),
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

/// String that may contain some expansions
///
/// A text is a sequence of [text unit](TextUnit)s, which may contain some kinds
/// of expansions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Text(pub Vec<TextUnit>);

/// Element of an [`EscapedString`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscapeUnit {
    /// Literal single character
    Literal(char),
    /// Backslash-escaped double-quote character (`\"`)
    DoubleQuote,
    /// Backslash-escaped single-quote character (`\'`)
    SingleQuote,
    /// Backslash-escaped backslash character (`\\`)
    Backslash,
    /// Backslash-escaped question mark character (`\?`)
    Question,
    /// Backslash notation for the bell character (`\a`, ASCII 7)
    Alert,
    /// Backslash notation for the backspace character (`\b`, ASCII 8)
    Backspace,
    /// Backslash notation for the escape character (`\e`, ASCII 27)
    Escape,
    /// Backslash notation for the form feed character (`\f`, ASCII 12)
    FormFeed,
    /// Backslash notation for the newline character (`\n`, ASCII 10)
    Newline,
    /// Backslash notation for the carriage return character (`\r`, ASCII 13)
    CarriageReturn,
    /// Backslash notation for the horizontal tab character (`\t`, ASCII 9)
    Tab,
    /// Backslash notation for the vertical tab character (`\v`, ASCII 11)
    VerticalTab,
    /// Control character notation (`\c...`)
    ///
    /// The associated value is the control character represented by the
    /// following character in the input.
    Control(u8),
    /// Single-byte octal notation (`\OOO`)
    ///
    /// The associated value is the byte represented by the three octal digits
    /// following the backslash.
    Octal(u8),
    /// Single-byte hexadecimal notation (`\xHH`)
    ///
    /// The associated value is the byte represented by the two hexadecimal
    /// digits following the `x`.
    Hex(u8),
    /// Unicode notation (`\uHHHH` or `\UHHHHHHHH`)
    ///
    /// The associated value is the Unicode scalar value represented by the four
    /// or eight hexadecimal digits following the `u` or `U`.
    Unicode(char),
}

/// String that may contain some escapes
///
/// An escaped string is a sequence of [escape unit](EscapeUnit)s, which may
/// contain some kinds of escapes. This type is used for the value of a
/// [dollar-single-quoted string](WordUnit::DollarSingleQuote).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EscapedString(pub Vec<EscapeUnit>);

/// Element of a [Word], i.e., text with quotes and tilde expansion
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WordUnit {
    /// Unquoted [`TextUnit`] as a word unit
    Unquoted(TextUnit),
    /// String surrounded with a pair of single quotations
    SingleQuote(String),
    /// Text surrounded with a pair of double quotations
    DoubleQuote(Text),
    /// String surrounded with a pair of single quotations and preceded by a dollar sign
    DollarSingleQuote(EscapedString),
    /// Tilde expansion
    ///
    /// The `String` value does not contain the initial tilde.
    Tilde(String),
}

pub use WordUnit::*;

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

/// Expansion style of a simple command word
///
/// TODO Elaborate
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpansionMode {
    /// Expand the word to a single field
    Single,
    /// Expand the word to multiple fields
    Multiple,
}

/// Command that involves assignments, redirections, and word expansions
///
/// In the shell language syntax, a valid simple command must contain at least one of assignments,
/// redirections, and words. The parser must not produce a completely empty simple command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimpleCommand {
    /// Assignments
    pub assigns: Vec<Assign>,
    /// Command name and arguments
    pub words: Vec<(Word, ExpansionMode)>,
    /// Redirections
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
        let Some((word, _)) = self.words.first() else {
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

/// Definitions and implementations of the [Unquote] and [MaybeLiteral] traits,
/// and other conversions between types
mod conversions;
/// Implementations of [std::fmt::Display] for the shell language syntax types
mod impl_display;

pub use conversions::{MaybeLiteral, NotLiteral, NotSpecialParam, Unquote};
