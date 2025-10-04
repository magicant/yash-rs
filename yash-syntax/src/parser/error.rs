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

//! Definition of errors that happen in the parser

use crate::source::Location;
#[allow(deprecated)]
use crate::source::pretty::{Annotation, AnnotationType, MessageBase};
use crate::syntax::AndOr;
use std::borrow::Cow;
use std::rc::Rc;
use thiserror::Error;

/// Types of syntax errors
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{}", self.message())]
#[non_exhaustive]
pub enum SyntaxError {
    /// A backslash is at the end of the input.
    IncompleteEscape,
    /// A backslash is not followed by a character that makes a valid escape.
    InvalidEscape,
    /// A `(` lacks a closing `)`.
    UnclosedParen { opening_location: Location },
    /// A single quotation lacks a closing `'`.
    UnclosedSingleQuote { opening_location: Location },
    /// A double quotation lacks a closing `"`.
    UnclosedDoubleQuote { opening_location: Location },
    /// A `$'` lacks a closing `'`.
    UnclosedDollarSingleQuote { opening_location: Location },
    /// A parameter expansion lacks a closing `}`.
    UnclosedParam { opening_location: Location },
    /// A parameter expansion lacks a name.
    EmptyParam,
    /// A parameter expansion has an invalid name.
    InvalidParam,
    /// A modifier does not have a valid form in a parameter expansion.
    InvalidModifier,
    /// A braced parameter expansion has both a prefix and suffix modifier.
    MultipleModifier,
    /// A command substitution started with `$(` but lacks a closing `)`.
    UnclosedCommandSubstitution { opening_location: Location },
    /// A command substitution started with `` ` `` but lacks a closing `` ` ``.
    UnclosedBackquote { opening_location: Location },
    /// An arithmetic expansion lacks a closing `))`.
    UnclosedArith { opening_location: Location },
    /// A command begins with an inappropriate keyword or operator token.
    InvalidCommandToken,
    /// A separator is missing between commands.
    MissingSeparator,
    /// The file descriptor specified for a redirection cannot be used.
    FdOutOfRange,
    /// An I/O location prefix attached to a redirection has an unsupported format.
    InvalidIoLocation,
    /// A redirection operator is missing its operand.
    MissingRedirOperand,
    /// A here-document operator is missing its delimiter token.
    MissingHereDocDelimiter,
    /// A here-document operator is missing its corresponding content.
    MissingHereDocContent,
    /// A here-document content is missing its delimiter.
    UnclosedHereDocContent { redir_op_location: Location },
    /// An array assignment started with `=(` but lacks a closing `)`.
    UnclosedArrayValue { opening_location: Location },
    /// A `}` appears without a matching `{`.
    UnopenedGrouping,
    /// A grouping is not closed.
    UnclosedGrouping { opening_location: Location },
    /// A grouping contains no commands.
    EmptyGrouping,
    /// A `)` appears without a matching `(`.
    UnopenedSubshell,
    /// A subshell is not closed.
    UnclosedSubshell { opening_location: Location },
    /// A subshell contains no commands.
    EmptySubshell,
    /// A `do` appears outside a loop.
    UnopenedLoop,
    /// A `done` appears outside a loop.
    UnopenedDoClause,
    /// A do clause is not closed.
    UnclosedDoClause { opening_location: Location },
    /// A do clause contains no commands.
    EmptyDoClause,
    /// The variable name is missing in a for loop.
    MissingForName,
    /// The variable name is not a valid word in a for loop.
    InvalidForName,
    /// A value is not a valid word in a for loop.
    InvalidForValue,
    /// A for loop is missing a do clause.
    MissingForBody { opening_location: Location },
    /// A while loop is missing a do clause.
    UnclosedWhileClause { opening_location: Location },
    /// A while loop's condition is empty.
    EmptyWhileCondition,
    /// An until loop is missing a do clause.
    UnclosedUntilClause { opening_location: Location },
    /// An until loop's condition is empty.
    EmptyUntilCondition,
    /// An if command is missing the then clause.
    IfMissingThen { if_location: Location },
    /// An if command's condition is empty.
    EmptyIfCondition,
    /// An if command's body is empty.
    EmptyIfBody,
    /// An elif clause is missing the then clause.
    ElifMissingThen { elif_location: Location },
    /// An elif clause's condition is empty.
    EmptyElifCondition,
    /// An elif clause's body is empty.
    EmptyElifBody,
    /// An else clause is empty.
    EmptyElse,
    /// An `elif`, `else`, `then`, or `fi` appears outside an if command.
    UnopenedIf,
    /// An if command is not closed.
    UnclosedIf { opening_location: Location },
    /// The case command is missing its subject.
    MissingCaseSubject,
    /// The subject of the case command is not a valid word.
    InvalidCaseSubject,
    /// The case command is missing `in` after the subject.
    MissingIn { opening_location: Location },
    /// The `)` is missing in a case item.
    UnclosedPatternList,
    /// The pattern is missing in a case item.
    MissingPattern,
    /// The pattern is not a valid word token.
    InvalidPattern,
    /// The first pattern of a case item is `esac`.
    #[deprecated = "this error no longer occurs"]
    EsacAsPattern,
    /// An `esac` or `;;` appears outside a case command.
    UnopenedCase,
    /// A case command is not closed.
    UnclosedCase { opening_location: Location },
    /// The `(` is not followed by `)` in a function definition.
    UnmatchedParenthesis,
    /// The function body is missing in a function definition command.
    MissingFunctionBody,
    /// A function body is not a compound command.
    InvalidFunctionBody,
    /// The keyword `in` is used as a command name.
    InAsCommandName,
    /// A pipeline is missing after a `&&` or `||` token.
    MissingPipeline(AndOr),
    /// Two successive `!` tokens.
    DoubleNegation,
    /// A `|` token is followed by a `!`.
    BangAfterBar,
    /// A command is missing after a `!` token.
    MissingCommandAfterBang,
    /// A command is missing after a `|` token.
    MissingCommandAfterBar,
    /// There is a redundant token.
    RedundantToken,
    /// A control escape (`\c...`) is incomplete in a dollar-single-quoted string.
    IncompleteControlEscape,
    /// A control-backslash escape (`\c\\`) is incomplete in a dollar-single-quoted string.
    IncompleteControlBackslashEscape,
    /// A control escape (`\c...`) does not have a valid control character.
    InvalidControlEscape,
    /// An octal escape is out of range (greater than `\377`) in a dollar-single-quoted string.
    OctalEscapeOutOfRange,
    /// An hexadecimal escape (`\x...`) is incomplete in a dollar-single-quoted string.
    IncompleteHexEscape,
    /// A Unicode escape (`\u...`) is incomplete in a dollar-single-quoted string.
    IncompleteShortUnicodeEscape,
    /// A Unicode escape (`\U...`) is incomplete in a dollar-single-quoted string.
    IncompleteLongUnicodeEscape,
    /// A Unicode escape (`\u...` or `\U...`) is out of range in a dollar-single-quoted string.
    UnicodeEscapeOutOfRange,
    /// The unsupported version of function definition syntax is used.
    UnsupportedFunctionDefinitionSyntax,
    /// A `[[ ... ]]` command is used.
    UnsupportedDoubleBracketCommand,
    /// A process redirection (`>(...)` or `<(...)`) is used.
    UnsupportedProcessRedirection,
}

impl SyntaxError {
    /// Returns an error message describing the error.
    #[must_use]
    pub fn message(&self) -> &'static str {
        use SyntaxError::*;
        match self {
            IncompleteEscape => "the backslash is escaping nothing",
            InvalidEscape => "the backslash escape is invalid",
            UnclosedParen { .. } => "the parenthesis is not closed",
            UnclosedSingleQuote { .. } => "the single quote is not closed",
            UnclosedDoubleQuote { .. } => "the double quote is not closed",
            UnclosedDollarSingleQuote { .. } => "the dollar single quote is not closed",
            UnclosedParam { .. } => "the parameter expansion is not closed",
            EmptyParam => "the parameter name is missing",
            InvalidParam => "the parameter name is invalid",
            InvalidModifier => "the parameter expansion contains a malformed modifier",
            MultipleModifier => "a suffix modifier cannot be used together with a prefix modifier",
            UnclosedCommandSubstitution { .. } => "the command substitution is not closed",
            UnclosedBackquote { .. } => "the backquote is not closed",
            UnclosedArith { .. } => "the arithmetic expansion is not closed",
            InvalidCommandToken => "the command starts with an inappropriate token",
            MissingSeparator => "a separator is missing between the commands",
            FdOutOfRange => "the file descriptor is too large",
            InvalidIoLocation => "the I/O location prefix is not valid",
            MissingRedirOperand => "the redirection operator is missing its operand",
            MissingHereDocDelimiter => "the here-document operator is missing its delimiter",
            MissingHereDocContent => "content of the here-document is missing",
            UnclosedHereDocContent { .. } => {
                "the delimiter to close the here-document content is missing"
            }
            UnclosedArrayValue { .. } => "the array assignment value is not closed",
            UnopenedGrouping | UnopenedSubshell | UnopenedLoop | UnopenedDoClause | UnopenedIf
            | UnopenedCase | InAsCommandName => "the compound command delimiter is unmatched",
            UnclosedGrouping { .. } => "the grouping is not closed",
            EmptyGrouping => "the grouping is missing its content",
            UnclosedSubshell { .. } => "the subshell is not closed",
            EmptySubshell => "the subshell is missing its content",
            UnclosedDoClause { .. } => "the `do` clause is missing its closing `done`",
            EmptyDoClause => "the `do` clause is missing its content",
            MissingForName => "the variable name is missing in the `for` loop",
            InvalidForName => "the variable name is invalid",
            InvalidForValue => "the operator token is invalid in the word list of the `for` loop",
            MissingForBody { .. } => "the `for` loop is missing its `do` clause",
            UnclosedWhileClause { .. } => "the `while` loop is missing its `do` clause",
            EmptyWhileCondition => "the `while` loop is missing its condition",
            UnclosedUntilClause { .. } => "the `until` loop is missing its `do` clause",
            EmptyUntilCondition => "the `until` loop is missing its condition",
            IfMissingThen { .. } => "the `if` command is missing the `then` clause",
            EmptyIfCondition => "the `if` command is missing its condition",
            EmptyIfBody => "the `if` command is missing its body",
            ElifMissingThen { .. } => "the `elif` clause is missing the `then` clause",
            EmptyElifCondition => "the `elif` clause is missing its condition",
            EmptyElifBody => "the `elif` clause is missing its body",
            EmptyElse => "the `else` clause is missing its content",
            UnclosedIf { .. } => "the `if` command is missing its closing `fi`",
            MissingCaseSubject => "the subject is missing after `case`",
            InvalidCaseSubject => "the `case` command subject is not a valid word",
            MissingIn { .. } => "`in` is missing in the `case` command",
            UnclosedPatternList => "the pattern list is not properly closed by a `)`",
            MissingPattern => "a pattern is missing in the `case` command",
            InvalidPattern => "the pattern is not a valid word token",
            #[allow(deprecated)]
            EsacAsPattern => "`esac` cannot be the first of a pattern list",
            UnclosedCase { .. } => "the `case` command is missing its closing `esac`",
            UnmatchedParenthesis => "`)` is missing after `(`",
            MissingFunctionBody => "the function body is missing",
            InvalidFunctionBody => "the function body must be a compound command",
            MissingPipeline(AndOr::AndThen) => "a command is missing after `&&`",
            MissingPipeline(AndOr::OrElse) => "a command is missing after `||`",
            DoubleNegation => "`!` cannot be used twice in a row",
            BangAfterBar => "`!` cannot be used in the middle of a pipeline",
            MissingCommandAfterBang => "a command is missing after `!`",
            MissingCommandAfterBar => "a command is missing after `|`",
            RedundantToken => "there is a redundant token",
            IncompleteControlEscape => "the control escape is incomplete",
            IncompleteControlBackslashEscape => "the control-backslash escape is incomplete",
            InvalidControlEscape => "the control escape is invalid",
            OctalEscapeOutOfRange => "the octal escape is out of range",
            IncompleteHexEscape => "the hexadecimal escape is incomplete",
            IncompleteShortUnicodeEscape | IncompleteLongUnicodeEscape => {
                "the Unicode escape is incomplete"
            }
            UnicodeEscapeOutOfRange => "the Unicode escape is out of range",
            UnsupportedFunctionDefinitionSyntax
            | UnsupportedDoubleBracketCommand
            | UnsupportedProcessRedirection => "unsupported syntax",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> &'static str {
        use SyntaxError::*;
        match self {
            IncompleteEscape => "expected an escaped character after the backslash",
            InvalidEscape => "invalid escape sequence",
            UnclosedParen { .. }
            | UnclosedCommandSubstitution { .. }
            | UnclosedArrayValue { .. }
            | UnclosedSubshell { .. }
            | UnclosedPatternList
            | UnmatchedParenthesis => "expected `)`",
            EmptyGrouping
            | EmptySubshell
            | EmptyDoClause
            | EmptyWhileCondition
            | EmptyUntilCondition
            | EmptyIfCondition
            | EmptyIfBody
            | EmptyElifCondition
            | EmptyElifBody
            | EmptyElse
            | MissingPipeline(_)
            | MissingCommandAfterBang
            | MissingCommandAfterBar => "expected a command",
            InvalidForValue | MissingCaseSubject | InvalidCaseSubject | MissingPattern
            | InvalidPattern => "expected a word",
            UnclosedSingleQuote { .. } | UnclosedDollarSingleQuote { .. } => "expected `'`",
            UnclosedDoubleQuote { .. } => "expected `\"`",
            UnclosedParam { .. } | UnclosedGrouping { .. } => "expected `}`",
            EmptyParam => "expected a parameter name",
            InvalidParam => "not a valid named or positional parameter",
            InvalidModifier => "broken modifier",
            MultipleModifier => "conflicting modifier",
            UnclosedBackquote { .. } => "expected '`'",
            UnclosedArith { .. } => "expected `))`",
            InvalidCommandToken => "does not begin a valid command",
            MissingSeparator => "expected `;` or `&` before this token",
            FdOutOfRange => "unsupported file descriptor",
            InvalidIoLocation => "unsupported I/O location prefix",
            MissingRedirOperand => "expected a redirection operand",
            MissingHereDocDelimiter => "expected a delimiter word",
            MissingHereDocContent => "content not found",
            UnclosedHereDocContent { .. } => "missing delimiter",
            UnopenedGrouping => "no grouping command to close",
            UnopenedSubshell => "no subshell to close",
            UnopenedLoop => "not in a loop",
            UnopenedDoClause => "no `do` clause to close",
            UnclosedDoClause { .. } => "expected `done`",
            MissingForName => "expected a variable name",
            InvalidForName => "not a valid variable name",
            MissingForBody { .. } | UnclosedWhileClause { .. } | UnclosedUntilClause { .. } => {
                "expected `do ... done`"
            }
            IfMissingThen { .. } | ElifMissingThen { .. } => "expected `then ... fi`",
            UnopenedIf => "not in an `if` command",
            UnclosedIf { .. } => "expected `fi`",
            MissingIn { .. } => "expected `in`",
            #[allow(deprecated)]
            EsacAsPattern => "needs quoting",
            UnopenedCase => "not in a `case` command",
            UnclosedCase { .. } => "expected `esac`",
            MissingFunctionBody | InvalidFunctionBody => "expected a compound command",
            InAsCommandName => "cannot be used as a command name",
            DoubleNegation => "only one `!` allowed",
            BangAfterBar => "`!` not allowed here",
            RedundantToken => "unexpected token",
            IncompleteControlEscape => r"expected a control character after `\c`",
            IncompleteControlBackslashEscape => r"expected another backslash after `\c\`",
            InvalidControlEscape => "not a valid control character",
            OctalEscapeOutOfRange => r"expected a value between \0 and \377",
            IncompleteHexEscape => r"expected a hexadecimal digit after `\x`",
            IncompleteShortUnicodeEscape => r"expected a hexadecimal digit after `\u`",
            IncompleteLongUnicodeEscape => r"expected a hexadecimal digit after `\U`",
            UnicodeEscapeOutOfRange => "not a valid Unicode scalar value",
            UnsupportedFunctionDefinitionSyntax => "the `function` keyword is not yet supported",
            UnsupportedDoubleBracketCommand => "the `[[ ... ]]` command is not yet supported",
            UnsupportedProcessRedirection => "process redirection is not yet supported",
        }
    }

    /// Returns a location related with the error cause and a message describing
    /// the location.
    #[must_use]
    pub fn related_location(&self) -> Option<(&Location, &'static str)> {
        use SyntaxError::*;
        match self {
            UnclosedParen { opening_location }
            | UnclosedSubshell { opening_location }
            | UnclosedArrayValue { opening_location } => {
                Some((opening_location, "the opening parenthesis was here"))
            }
            UnclosedSingleQuote { opening_location }
            | UnclosedDoubleQuote { opening_location }
            | UnclosedDollarSingleQuote { opening_location } => {
                Some((opening_location, "the opening quote was here"))
            }
            UnclosedParam { opening_location } => {
                Some((opening_location, "the parameter started here"))
            }
            UnclosedCommandSubstitution { opening_location } => {
                Some((opening_location, "the command substitution started here"))
            }
            UnclosedBackquote { opening_location } => {
                Some((opening_location, "the opening backquote was here"))
            }
            UnclosedArith { opening_location } => {
                Some((opening_location, "the arithmetic expansion started here"))
            }
            UnclosedHereDocContent { redir_op_location } => {
                Some((redir_op_location, "the redirection operator was here"))
            }
            UnclosedGrouping { opening_location } => {
                Some((opening_location, "the opening brace was here"))
            }
            UnclosedDoClause { opening_location } => {
                Some((opening_location, "the `do` clause started here"))
            }
            MissingForBody { opening_location } => {
                Some((opening_location, "the `for` loop started here"))
            }
            UnclosedWhileClause { opening_location } => {
                Some((opening_location, "the `while` loop started here"))
            }
            UnclosedUntilClause { opening_location } => {
                Some((opening_location, "the `until` loop started here"))
            }
            IfMissingThen { if_location }
            | UnclosedIf {
                opening_location: if_location,
            } => Some((if_location, "the `if` command started here")),
            ElifMissingThen { elif_location } => {
                Some((elif_location, "the `elif` clause started here"))
            }
            MissingIn { opening_location } | UnclosedCase { opening_location } => {
                Some((opening_location, "the `case` command started here"))
            }
            _ => None,
        }
    }
}

/// Types of errors that may happen in parsing
#[derive(Clone, Debug, Error)]
#[error("{}", self.message())]
pub enum ErrorCause {
    /// Error in an underlying input function
    Io(#[from] Rc<std::io::Error>),
    /// Syntax error
    Syntax(#[from] SyntaxError),
}

impl PartialEq for ErrorCause {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ErrorCause::Syntax(e1), ErrorCause::Syntax(e2)) => e1 == e2,
            _ => false,
        }
    }
}

impl ErrorCause {
    /// Returns an error message describing the error cause.
    #[must_use]
    pub fn message(&self) -> Cow<'static, str> {
        use ErrorCause::*;
        match self {
            Io(e) => format!("cannot read commands: {e}").into(),
            Syntax(e) => e.message().into(),
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> &'static str {
        use ErrorCause::*;
        match self {
            Io(_) => "the command could be read up to here",
            Syntax(e) => e.label(),
        }
    }

    /// Returns a location related with the error cause and a message describing
    /// the location.
    #[must_use]
    pub fn related_location(&self) -> Option<(&Location, &'static str)> {
        use ErrorCause::*;
        match self {
            Io(_) => None,
            Syntax(e) => e.related_location(),
        }
    }
}

impl From<std::io::Error> for ErrorCause {
    fn from(e: std::io::Error) -> ErrorCause {
        ErrorCause::from(Rc::new(e))
    }
}

/// Explanation of a failure in parsing
#[derive(Clone, Debug, Error, PartialEq)]
#[error("{cause}")]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

#[allow(deprecated)]
impl MessageBase for Error {
    fn message_title(&self) -> Cow<'_, str> {
        self.cause.message()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            self.cause.label().into(),
            &self.location,
        )
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        // TODO Use Extend::extend_one
        if let Some((location, label)) = self.cause.related_location() {
            results.extend(std::iter::once(Annotation::new(
                AnnotationType::Info,
                label.into(),
                location,
            )));
        }
        if let ErrorCause::Syntax(SyntaxError::BangAfterBar) = &self.cause {
            results.extend(std::iter::once(Annotation::new(
                AnnotationType::Help,
                "surround this in a grouping: `{ ! ...; }`".into(),
                &self.location,
            )));
        }
    }
}

#[allow(deprecated)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Code;
    use crate::source::Source;
    use crate::source::pretty::Message;
    use std::num::NonZeroU64;
    use std::rc::Rc;

    #[test]
    fn display_for_error() {
        let code = Rc::new(Code {
            value: "".to_string().into(),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown.into(),
        });
        let location = Location { code, range: 0..42 };
        let error = Error {
            cause: SyntaxError::MissingHereDocDelimiter.into(),
            location,
        };
        assert_eq!(
            error.to_string(),
            "the here-document operator is missing its delimiter"
        );
    }

    #[test]
    fn from_error_for_message() {
        let code = Rc::new(Code {
            value: "".to_string().into(),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown.into(),
        });
        let location = Location { code, range: 0..42 };
        let error = Error {
            cause: SyntaxError::MissingHereDocDelimiter.into(),
            location,
        };
        let message = Message::from(&error);
        assert_eq!(message.r#type, AnnotationType::Error);
        assert_eq!(
            message.title,
            "the here-document operator is missing its delimiter"
        );
        assert_eq!(message.annotations.len(), 1);
        assert_eq!(message.annotations[0].r#type, AnnotationType::Error);
        assert_eq!(message.annotations[0].label, "expected a delimiter word");
        assert_eq!(message.annotations[0].location, &error.location);
    }
}
