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
//! This module contains types that represent abstract syntax trees (ASTs) of the shell language.
//!
//! Some types in this module has the type parameter `<H = HereDoc>`. As a user of the AST, you
//! will never have to specify the parameter other than the default `HereDoc`. The parameter is
//! used by the parser to create intermediate ASTs that lack sub-trees for here-documents, since
//! the contents of here-documents have to be parsed separately from the normal flow of source code.
//!
//! TODO Elaborate

use crate::parser::lex::Operator;
use crate::source::Location;
use itertools::Itertools;
use std::convert::TryFrom;
use std::fmt;
use std::os::unix::io::RawFd;
use std::rc::Rc;

/// Possibly literal syntax element.
///
/// When an instance of an implementor is literal, it can be converted directly
/// to a string.
pub trait MaybeLiteral {
    /// Checks if `self` is literal and, if so, converts to a string.
    fn to_string_if_literal(&self) -> Option<String>;
}

/// Element of a [Word] that can be double-quoted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DoubleQuotable {
    /// Literal single character.
    Literal(char),
    /// Backslash-escaped single character.
    Backslashed(char),
    // Parameter(TODO),
    /// Command substitution of the form `$(...)`.
    CommandSubst { content: String, location: Location },
    // Backquote(TODO),
    // Arith(TODO),
}

pub use DoubleQuotable::*;

impl fmt::Display for DoubleQuotable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal(c) => write!(f, "{}", c),
            Backslashed(c) => write!(f, "\\{}", c),
            CommandSubst { content, .. } => write!(f, "$({})", content),
        }
    }
}

impl MaybeLiteral for DoubleQuotable {
    /// If `self` is `Literal`, returns the character converted to a string.
    /// Otherwise, returns `None`.
    fn to_string_if_literal(&self) -> Option<String> {
        if let Literal(c) = self {
            Some(c.to_string())
        } else {
            None
        }
    }
}

/// Element of a [Word].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WordUnit {
    /// Unquoted [`DoubleQuotable`] as a word unit.
    Unquoted(DoubleQuotable),
    // TODO DoubleQuote(Vec<DoubleQuotable>),
    // TODO SingleQuote(String),
}

pub use WordUnit::*;

impl fmt::Display for WordUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unquoted(ref dq) => write!(f, "{}", dq),
        }
    }
}

impl MaybeLiteral for WordUnit {
    /// If `self` is `Unquoted(Literal(_))`, returns the character converted to a
    /// string. Otherwise, returns `None`.
    fn to_string_if_literal(&self) -> Option<String> {
        let Unquoted(dq) = self;
        dq.to_string_if_literal()
        // if let Unquoted(dq) = self {
        //     dq.to_string_if_literal()
        // } else {
        //     None
        // }
    }
}

impl MaybeLiteral for [WordUnit] {
    /// Converts the word units to a string if all the word units are literal,
    /// that is, `WordUnit::Unquoted(DoubleQuotable::Literal(_))`.
    fn to_string_if_literal(&self) -> Option<String> {
        fn try_to_char(u: &WordUnit) -> Option<char> {
            if let Unquoted(Literal(c)) = u {
                Some(*c)
            } else {
                None
            }
        }
        self.iter().map(try_to_char).collect()
    }
}

/// Token that may involve expansion.
///
/// It depends on context whether an empty word is valid or not. It is your responsibility to
/// ensure a word is non-empty in a context where it cannot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Word {
    /// Word units that constitute the word.
    pub units: Vec<WordUnit>,
    /// Location of the first character of the word.
    pub location: Location,
}

impl Word {
    /// Creates a constant word with unknown source.
    ///
    /// This is a convenience function to make a simple word, mainly for debugging
    /// purpose.
    ///
    /// The resulting word elements are not quoted even if they would be usually special.
    pub fn with_str(s: String) -> Word {
        let mut units = vec![];
        for c in s.chars() {
            units.push(WordUnit::Unquoted(DoubleQuotable::Literal(c)));
        }
        Word {
            units,
            location: Location::dummy(s),
        }
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.units.iter().try_for_each(|unit| write!(f, "{}", unit))
    }
}

impl MaybeLiteral for Word {
    /// Converts the word to a string if the word is fully literal, that is, all composed of
    /// `WordUnit::Unquoted(DoubleQuotable::Literal(_))`.
    fn to_string_if_literal(&self) -> Option<String> {
        self.units.to_string_if_literal()
    }
}

/// Value of an [assignment](Assign).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// Scalar value, a possibly empty word.
    ///
    /// Note: Because a scalar assignment value is created from a normal command
    /// word, the location of the word in the scalar value points to the first
    /// character of the entire assignment word rather than the assigned value.
    Scalar(Word),
    /// Array, possibly empty list of non-empty words.
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

impl Assign {
    /// Creates an assignment with unknown source.
    ///
    /// This is a convenience function to make a simple scalar assignment, mainly
    /// for debugging purpose. The assigned value is created with
    /// [`Word::with_str`].
    pub fn dummy(name: String, value: String) -> Assign {
        let line = format!("{}={}", &name, &value);
        Assign {
            name,
            value: Scalar(Word::with_str(value)),
            location: Location::dummy(line),
        }
    }
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
                    // TODO parse tilde expansions in the value
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
    /// is quoted, the content must not contain any expansion.
    pub content: Word,
}

impl fmt::Display for HereDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.remove_tabs { "<<-" } else { "<<" })?;

        // This space is to disambiguate `<< --` and `<<- -`
        if let Some(Unquoted(Literal('-'))) = self.delimiter.units.get(0) {
            f.write_str(" ")?;
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

impl fmt::Display for RedirBody {
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

impl fmt::Display for Redir {
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
}

impl fmt::Display for SimpleCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i1 = self.assigns.iter().map(|x| x as &dyn fmt::Display);
        let i2 = self.words.iter().map(|x| x as &dyn fmt::Display);
        let i3 = self.redirs.iter().map(|x| x as &dyn fmt::Display);
        write!(f, "{}", i1.chain(i2).chain(i3).format(" "))
        // TODO Avoid printing a keyword as the first word
    }
}

/// Element of a pipe sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command<H = HereDoc> {
    /// Simple command.
    SimpleCommand(SimpleCommand<H>),
    // TODO Compound command
    // TODO Function definition
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::SimpleCommand(c) => write!(f, "{}", c),
        }
    }
}

/// Commands separated by `|`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pipeline<H = HereDoc> {
    /// Elements of the pipeline.
    ///
    /// A valid pipeline must have at least one command.
    pub commands: Vec<Rc<Command<H>>>,
    /// True if the pipeline begins with a `!`.
    pub negation: bool,
}

impl fmt::Display for Pipeline {
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

impl fmt::Display for AndOrList {
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
impl fmt::Display for Item {
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct List<H = HereDoc> {
    /// Elements of the list.
    ///
    /// It depends on context whether an empty vector is a valid syntax.
    pub items: Vec<Item<H>>,
}

/// Allows conversion from List to String.
///
/// By default, the last `;` terminator is omitted from the formatted string.
/// When the alternate flag is specified as in `{:#}`, the result is always
/// terminated by either `;` or `&`.
impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((last, others)) = self.items.split_last() {
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

    #[test]
    fn double_quotable_display() {
        let literal = Literal('A');
        assert_eq!(literal.to_string(), "A");
        let backslashed = Backslashed('X');
        assert_eq!(backslashed.to_string(), r"\X");
    }

    #[test]
    fn word_to_string_if_literal_success() {
        let empty = Word::with_str("".to_string());
        let s = empty.to_string_if_literal().unwrap();
        assert_eq!(s, "");

        let nonempty = Word::with_str("foo".to_string());
        let s = nonempty.to_string_if_literal().unwrap();
        assert_eq!(s, "foo");
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
    }

    #[test]
    fn scalar_display() {
        let s = Scalar(Word::with_str("my scalar value".to_string()));
        assert_eq!(s.to_string(), "my scalar value");
    }

    #[test]
    fn array_display_empty() {
        let a = Array(vec![]);
        assert_eq!(a.to_string(), "()");
    }

    #[test]
    fn array_display_one() {
        let a = Array(vec![Word::with_str("one".to_string())]);
        assert_eq!(a.to_string(), "(one)");
    }

    #[test]
    fn array_display_many() {
        let a = Array(vec![
            Word::with_str("let".to_string()),
            Word::with_str("me".to_string()),
            Word::with_str("see".to_string()),
        ]);
        assert_eq!(a.to_string(), "(let me see)");
    }

    #[test]
    fn assign_display() {
        let mut a = Assign::dummy("foo".to_string(), "bar".to_string());
        assert_eq!(a.to_string(), "foo=bar");

        a.value = Array(vec![]);
        assert_eq!(a.to_string(), "foo=()");
    }

    #[test]
    fn assign_try_from_word_without_equal() {
        let word = Word::with_str("foo".to_string());
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_empty_name() {
        let word = Word::with_str("=foo".to_string());
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_non_literal_name() {
        let mut word = Word::with_str("night=foo".to_string());
        word.units.insert(0, Unquoted(Backslashed('k')));
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_literal_name() {
        let word = Word::with_str("night=foo".to_string());
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
            delimiter: Word::with_str("END".to_string()),
            remove_tabs: true,
            content: Word::with_str("here".to_string()),
        };
        assert_eq!(heredoc.to_string(), "<<-END");

        let heredoc = HereDoc {
            delimiter: Word::with_str("XXX".to_string()),
            remove_tabs: false,
            content: Word::with_str("there".to_string()),
        };
        assert_eq!(heredoc.to_string(), "<<XXX");
    }

    #[test]
    fn here_doc_display_disambiguation() {
        let heredoc = HereDoc {
            delimiter: Word::with_str("--".to_string()),
            remove_tabs: false,
            content: Word::with_str("here".to_string()),
        };
        assert_eq!(heredoc.to_string(), "<< --");

        let heredoc = HereDoc {
            delimiter: Word::with_str("-".to_string()),
            remove_tabs: true,
            content: Word::with_str("here".to_string()),
        };
        assert_eq!(heredoc.to_string(), "<<- -");
    }

    #[test]
    fn redir_display() {
        let heredoc = HereDoc {
            delimiter: Word::with_str("END".to_string()),
            remove_tabs: false,
            content: Word::with_str("here".to_string()),
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
            .push(Assign::dummy("name".to_string(), "value".to_string()));
        assert_eq!(command.to_string(), "name=value");

        command
            .assigns
            .push(Assign::dummy("hello".to_string(), "world".to_string()));
        assert_eq!(command.to_string(), "name=value hello=world");

        command.words.push(Word::with_str("echo".to_string()));
        assert_eq!(command.to_string(), "name=value hello=world echo");

        command.words.push(Word::with_str("foo".to_string()));
        assert_eq!(command.to_string(), "name=value hello=world echo foo");

        command.redirs.push(Redir {
            fd: None,
            body: RedirBody::from(HereDoc {
                delimiter: Word::with_str("END".to_string()),
                remove_tabs: false,
                content: Word::with_str("".to_string()),
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
                delimiter: Word::with_str("here".to_string()),
                remove_tabs: true,
                content: Word::with_str("ignored".to_string()),
            }),
        });
        assert_eq!(command.to_string(), "<<END 1<<-here");

        command
            .assigns
            .push(Assign::dummy("foo".to_string(), "bar".to_string()));
        assert_eq!(command.to_string(), "foo=bar <<END 1<<-here");
    }

    fn dummy_command(s: String) -> Rc<Command> {
        let w = Word::with_str(s);
        let s = SimpleCommand {
            assigns: vec![],
            words: vec![w],
            redirs: vec![],
        };
        Rc::new(Command::SimpleCommand(s))
    }

    fn dummy_pipeline(s: String) -> Pipeline {
        let c = dummy_command(s);
        Pipeline {
            commands: vec![c],
            negation: false,
        }
    }

    fn dummy_and_or_list(s: String) -> AndOrList {
        let first = dummy_pipeline(s);
        AndOrList {
            first,
            rest: vec![],
        }
    }

    #[test]
    fn pipeline_display() {
        let mut p = Pipeline {
            commands: vec![],
            negation: false,
        };
        p.commands.push(dummy_command("first".to_string()));
        assert_eq!(p.to_string(), "first");

        p.negation = true;
        assert_eq!(p.to_string(), "! first");

        p.commands.push(dummy_command("second".to_string()));
        assert_eq!(p.to_string(), "! first | second");

        p.commands.push(dummy_command("third".to_string()));
        p.negation = false;
        assert_eq!(p.to_string(), "first | second | third");
    }

    #[test]
    fn and_or_list_display() {
        let p = dummy_pipeline("first".to_string());
        let mut aol = AndOrList {
            first: p,
            rest: vec![],
        };
        assert_eq!(aol.to_string(), "first");

        let p = dummy_pipeline("second".to_string());
        aol.rest.push((AndOr::AndThen, p));
        assert_eq!(aol.to_string(), "first && second");

        let p = dummy_pipeline("third".to_string());
        aol.rest.push((AndOr::OrElse, p));
        assert_eq!(aol.to_string(), "first && second || third");
    }

    #[test]
    fn list_display() {
        let and_or = dummy_and_or_list("first".to_string());
        let item = Item {
            and_or,
            is_async: false,
        };
        let mut list = List { items: vec![item] };
        assert_eq!(list.to_string(), "first");

        let and_or = dummy_and_or_list("second".to_string());
        let item = Item {
            and_or,
            is_async: true,
        };
        list.items.push(item);
        assert_eq!(list.to_string(), "first; second&");

        let and_or = dummy_and_or_list("third".to_string());
        let item = Item {
            and_or,
            is_async: false,
        };
        list.items.push(item);
        assert_eq!(list.to_string(), "first; second& third");
    }

    #[test]
    fn list_display_alternate() {
        let and_or = dummy_and_or_list("first".to_string());
        let item = Item {
            and_or,
            is_async: false,
        };
        let mut list = List { items: vec![item] };
        assert_eq!(format!("{:#}", list), "first;");

        let and_or = dummy_and_or_list("second".to_string());
        let item = Item {
            and_or,
            is_async: true,
        };
        list.items.push(item);
        assert_eq!(format!("{:#}", list), "first; second&");

        let and_or = dummy_and_or_list("third".to_string());
        let item = Item {
            and_or,
            is_async: false,
        };
        list.items.push(item);
        assert_eq!(format!("{:#}", list), "first; second& third;");
    }
}
