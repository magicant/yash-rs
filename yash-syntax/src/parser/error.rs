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

//! Definition of errors that happen in the parser.

use crate::source::pretty::Annotation;
use crate::source::pretty::AnnotationType;
use crate::source::pretty::MessageBase;
use crate::source::Location;
use crate::syntax::AndOr;
use std::borrow::Cow;
use std::rc::Rc;
use thiserror::Error;

/// Types of syntax errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{}", self.message())]
#[non_exhaustive]
pub enum SyntaxError {
    /// A `(` lacks a closing `)`.
    UnclosedParen { opening_location: Location },
    /// A modifier does not have a valid form in a parameter expansion.
    InvalidModifier,
    /// A braced parameter expansion has both a prefix and suffix modifier.
    MultipleModifier,
    /// A single quotation lacks a closing `'`.
    UnclosedSingleQuote { opening_location: Location },
    /// A double quotation lacks a closing `"`.
    UnclosedDoubleQuote { opening_location: Location },
    /// A parameter expansion lacks a closing `}`.
    UnclosedParam { opening_location: Location },
    /// A parameter expansion lacks a name.
    EmptyParam,
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
}

impl SyntaxError {
    /// Returns an error message describing the error.
    #[must_use]
    pub fn message(&self) -> &'static str {
        use SyntaxError::*;
        match self {
            UnclosedParen { .. } => "The parenthesis is not closed",
            InvalidModifier => "The parameter expansion contains a malformed modifier",
            MultipleModifier => "A suffix modifier cannot be used together with a prefix modifier",
            UnclosedSingleQuote { .. } => "The single quote is not closed",
            UnclosedDoubleQuote { .. } => "The double quote is not closed",
            UnclosedParam { .. } => "The parameter expansion is not closed",
            EmptyParam => "The parameter name is missing",
            UnclosedCommandSubstitution { .. } => "The command substitution is not closed",
            UnclosedBackquote { .. } => "The backquote is not closed",
            UnclosedArith { .. } => "The arithmetic expansion is not closed",
            InvalidCommandToken => "The command starts with an inappropriate token",
            MissingSeparator => "A separator is missing between the commands",
            FdOutOfRange => "The file descriptor is too large",
            MissingRedirOperand => "The redirection operator is missing its operand",
            MissingHereDocDelimiter => "The here-document operator is missing its delimiter",
            MissingHereDocContent => "Content of the here-document is missing",
            UnclosedHereDocContent { .. } => {
                "The delimiter to close the here-document content is missing"
            }
            UnclosedArrayValue { .. } => "The array assignment value is not closed",
            UnopenedGrouping | UnopenedSubshell | UnopenedLoop | UnopenedDoClause | UnopenedIf
            | UnopenedCase | InAsCommandName => "The compound command delimiter is unmatched",
            UnclosedGrouping { .. } => "The grouping is not closed",
            EmptyGrouping => "The grouping is missing its content",
            UnclosedSubshell { .. } => "The subshell is not closed",
            EmptySubshell => "The subshell is missing its content",
            UnclosedDoClause { .. } => "The `do` clause is missing its closing `done`",
            EmptyDoClause => "The `do` clause is missing its content",
            MissingForName => "The variable name is missing in the `for` loop",
            InvalidForName => "The variable name is invalid",
            InvalidForValue => "The operator token is invalid in the word list of the `for` loop",
            MissingForBody { .. } => "The `for` loop is missing its `do` clause",
            UnclosedWhileClause { .. } => "The `while` loop is missing its `do` clause",
            EmptyWhileCondition => "The `while` loop is missing its condition",
            UnclosedUntilClause { .. } => "The `until` loop is missing its `do` clause",
            EmptyUntilCondition => "The `until` loop is missing its condition",
            IfMissingThen { .. } => "The `if` command is missing the `then` clause",
            EmptyIfCondition => "The `if` command is missing its condition",
            EmptyIfBody => "The `if` command is missing its body",
            ElifMissingThen { .. } => "The `elif` clause is missing the `then` clause",
            EmptyElifCondition => "The `elif` clause is missing its condition",
            EmptyElifBody => "The `elif` clause is missing its body",
            EmptyElse => "The `else` clause is missing its content",
            UnclosedIf { .. } => "The `if` command is missing its closing `fi`",
            MissingCaseSubject => "The subject is missing after `case`",
            InvalidCaseSubject => "The `case` command subject is not a valid word",
            MissingIn { .. } => "`in` is missing in the `case` command",
            UnclosedPatternList => "The pattern list is not properly closed by a `)`",
            MissingPattern => "A pattern is missing in the `case` command",
            InvalidPattern => "The pattern is not a valid word token",
            EsacAsPattern => "`esac` cannot be the first of a pattern list",
            UnclosedCase { .. } => "The `case` command is missing its closing `esac`",
            UnmatchedParenthesis => "`)` is missing after `(`",
            MissingFunctionBody => "The function body is missing",
            InvalidFunctionBody => "The function body must be a compound command",
            MissingPipeline(AndOr::AndThen) => "A command is missing after `&&`",
            MissingPipeline(AndOr::OrElse) => "A command is missing after `||`",
            DoubleNegation => "`!` cannot be used twice in a row",
            BangAfterBar => "`!` cannot be used in the middle of a pipeline",
            MissingCommandAfterBang => "A command is missing after `!`",
            MissingCommandAfterBar => "A command is missing after `|`",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> &'static str {
        use SyntaxError::*;
        match self {
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
            InvalidModifier => "broken modifier",
            MultipleModifier => "conflicting modifier",
            UnclosedSingleQuote { .. } => "expected `'`",
            UnclosedDoubleQuote { .. } => "expected `\"`",
            UnclosedParam { .. } | UnclosedGrouping { .. } => "expected `}`",
            EmptyParam => "expected a parameter name",
            UnclosedBackquote { .. } => "expected '`'",
            UnclosedArith { .. } => "expected `))`",
            InvalidCommandToken => "does not begin a valid command",
            MissingSeparator => "expected `;` or `&` before this token",
            FdOutOfRange => "unsupported file descriptor",
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
            EsacAsPattern => "needs quoting",
            UnopenedCase => "not in a `case` command",
            UnclosedCase { .. } => "expected `esac`",
            MissingFunctionBody | InvalidFunctionBody => "expected a compound command",
            InAsCommandName => "cannot be used as a command name",
            DoubleNegation => "only one `!` allowed",
            BangAfterBar => "`!` not allowed here",
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
            UnclosedSingleQuote { opening_location } | UnclosedDoubleQuote { opening_location } => {
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

/// Types of errors that may happen in parsing.
#[derive(Clone, Debug, Error)]
#[error("{}", self.message())]
pub enum ErrorCause {
    /// Error in an underlying input function.
    Io(#[from] Rc<std::io::Error>),
    /// Syntax error.
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

/// Explanation of a failure in parsing.
#[derive(Clone, Debug, Error, PartialEq)]
#[error("{cause}")]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        self.cause.message()
    }

    fn main_annotation(&self) -> Annotation {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::pretty::Message;
    use crate::source::Code;
    use crate::source::Source;
    use std::num::NonZeroU64;
    use std::rc::Rc;

    #[test]
    fn display_for_error() {
        let code = Rc::new(Code {
            value: "".to_string().into(),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown,
        });
        let location = Location { code, range: 0..42 };
        let error = Error {
            cause: SyntaxError::MissingHereDocDelimiter.into(),
            location,
        };
        assert_eq!(
            error.to_string(),
            "The here-document operator is missing its delimiter"
        );
    }

    #[test]
    fn from_error_for_message() {
        let code = Rc::new(Code {
            value: "".to_string().into(),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown,
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
            "The here-document operator is missing its delimiter"
        );
        assert_eq!(message.annotations.len(), 1);
        assert_eq!(message.annotations[0].r#type, AnnotationType::Error);
        assert_eq!(message.annotations[0].label, "expected a delimiter word");
        assert_eq!(message.annotations[0].location, &error.location);
    }
}
