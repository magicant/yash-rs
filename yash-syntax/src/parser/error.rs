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

use crate::source::Location;
use crate::syntax::AndOr;
use std::borrow::Cow;
use std::fmt;
use std::rc::Rc;

/// Types of syntax errors.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    // TODO Should we remove `UnexpectedToken` in favor of other error types?
    /// Unexpected token.
    UnexpectedToken,
    /// The file descriptor specified for a redirection cannot be used.
    FdOutOfRange,
    /// A redirection operator is missing its operand.
    MissingRedirOperand,
    /// A here-document operator is missing its delimiter token.
    MissingHereDocDelimiter,
    // TODO Include the corresponding here-doc operator.
    /// A here-document operator is missing its corresponding content.
    MissingHereDocContent,
    /// A here-document content is missing its delimiter.
    UnclosedHereDocContent { redir_op_location: Location },
    /// An array assignment started with `=(` but lacks a closing `)`.
    UnclosedArrayValue { opening_location: Location },
    /// A grouping is not closed.
    UnclosedGrouping { opening_location: Location },
    /// A grouping contains no commands.
    EmptyGrouping,
    /// A subshell is not closed.
    UnclosedSubshell { opening_location: Location },
    /// A subshell contains no commands.
    EmptySubshell,
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
    /// A case command is not closed.
    UnclosedCase { opening_location: Location },
    /// The `(` is not followed by `)` in a function definition.
    UnmatchedParenthesis,
    /// The function body is missing in a function definition command.
    MissingFunctionBody,
    /// A function body is not a compound command.
    InvalidFunctionBody,
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
            UnexpectedToken => "Unexpected token",
            FdOutOfRange => "The file descriptor is too large",
            MissingRedirOperand => "The redirection operator is missing its operand",
            MissingHereDocDelimiter => "The here-document operator is missing its delimiter",
            MissingHereDocContent => "Content of the here-document is missing",
            UnclosedHereDocContent { .. } => {
                "The delimiter to close the here-document content is missing"
            }
            UnclosedArrayValue { .. } => "The array assignment value is not closed",
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
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

/// Types of errors that may happen in parsing.
#[derive(Clone, Debug)]
pub enum ErrorCause {
    /// Error in an underlying input function.
    Io(Rc<std::io::Error>),
    /// Syntax error.
    Syntax(SyntaxError),
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
            Io(e) => format!("cannot read commands: {}", e).into(),
            Syntax(e) => e.message().into(),
        }
    }
}

impl fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message().fmt(f)
    }
}

impl From<Rc<std::io::Error>> for ErrorCause {
    fn from(e: Rc<std::io::Error>) -> ErrorCause {
        ErrorCause::Io(e)
    }
}

impl From<std::io::Error> for ErrorCause {
    fn from(e: std::io::Error) -> ErrorCause {
        ErrorCause::from(Rc::new(e))
    }
}

impl From<SyntaxError> for ErrorCause {
    fn from(e: SyntaxError) -> ErrorCause {
        ErrorCause::Syntax(e)
    }
}

/// Explanation of a failure in parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)
        // TODO Print Location
    }
}

// TODO Consider implementing std::error::Error for self::Error

/// Converts an `Error` to an annotated snippet.
///
/// This implementation is available only when the `"annotate-snippets"` feature
/// is enabled.
#[cfg(feature = "annotate-snippets")]
impl<'a> From<&'a Error> for annotate_snippets::snippet::Snippet<'a> {
    fn from(error: &'a Error) -> Self {
        use annotate_snippets::snippet::*;
        use std::convert::TryInto;

        let index = error.location.column.get().try_into().unwrap_or(usize::MAX);

        Snippet {
            title: Some(Annotation {
                label: Some("parser error"), // TODO correct message
                id: None,
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices: vec![Slice {
                source: &error.location.line.value,
                line_start: error
                    .location
                    .line
                    .number
                    .get()
                    .try_into()
                    .unwrap_or(usize::MAX),
                origin: Some("<origin>"), // TODO correct origin
                fold: false,
                annotations: vec![SourceAnnotation {
                    label: "",
                    annotation_type: AnnotationType::Error,
                    range: (index - 1, index),
                }],
            }],
            opt: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Line;
    use crate::source::Source;
    use std::num::NonZeroU64;
    use std::rc::Rc;

    #[test]
    fn display_for_error() {
        let number = NonZeroU64::new(1).unwrap();
        let line = Rc::new(Line {
            value: "".to_string(),
            number,
            source: Source::Unknown,
        });
        let location = Location {
            line,
            column: number,
        };
        let error = Error {
            cause: SyntaxError::MissingHereDocDelimiter.into(),
            location,
        };
        assert_eq!(
            error.to_string(),
            "The here-document operator is missing its delimiter"
        );
    }
}
