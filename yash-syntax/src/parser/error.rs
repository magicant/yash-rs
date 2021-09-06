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

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SyntaxError::*;
        match self {
            UnclosedParen { .. } => f.write_str("The parenthesis is not closed"),
            InvalidModifier => f.write_str("The parameter expansion contains a malformed modifier"),
            MultipleModifier => {
                f.write_str("A suffix modifier cannot be used together with a prefix modifier")
            }
            UnclosedSingleQuote { .. } => f.write_str("The single quote is not closed"),
            UnclosedDoubleQuote { .. } => f.write_str("The double quote is not closed"),
            UnclosedParam { .. } => f.write_str("The parameter expansion is not closed"),
            EmptyParam => f.write_str("The parameter name is missing"),
            UnclosedCommandSubstitution { .. } => {
                f.write_str("The command substitution is not closed")
            }
            UnclosedBackquote { .. } => f.write_str("The backquote is not closed"),
            UnclosedArith { .. } => f.write_str("The arithmetic expansion is not closed"),
            UnexpectedToken => f.write_str("Unexpected token"),
            FdOutOfRange => f.write_str("The file descriptor is too large"),
            MissingRedirOperand => f.write_str("The redirection operator is missing its operand"),
            MissingHereDocDelimiter => {
                f.write_str("The here-document operator is missing its delimiter")
            }
            MissingHereDocContent => f.write_str("Content of the here-document is missing"),
            UnclosedHereDocContent { .. } => {
                f.write_str("The delimiter to close the here-document content is missing")
            }
            UnclosedArrayValue { .. } => f.write_str("The array assignment value is not closed"),
            UnclosedGrouping { .. } => f.write_str("The grouping is not closed"),
            EmptyGrouping => f.write_str("The grouping is missing its content"),
            UnclosedSubshell { .. } => f.write_str("The subshell is not closed"),
            EmptySubshell => f.write_str("The subshell is missing its content"),
            UnclosedDoClause { .. } => f.write_str("The `do` clause is missing its closing `done`"),
            EmptyDoClause => f.write_str("The `do` clause is missing its content"),
            MissingForName => f.write_str("The variable name is missing in the `for` loop"),
            InvalidForName => f.write_str("The variable name is invalid"),
            InvalidForValue => {
                f.write_str("The operator token is invalid in the word list of the `for` loop")
            }
            MissingForBody { .. } => f.write_str("The `for` loop is missing its `do` clause"),
            UnclosedWhileClause { .. } => {
                f.write_str("The `while` loop is missing its `do` clause")
            }
            EmptyWhileCondition => f.write_str("The `while` loop is missing its condition"),
            UnclosedUntilClause { .. } => {
                f.write_str("The `until` loop is missing its `do` clause")
            }
            EmptyUntilCondition => f.write_str("The `until` loop is missing its condition"),
            IfMissingThen { .. } => f.write_str("The `if` command is missing the `then` clause"),
            EmptyIfCondition => f.write_str("The `if` command is missing its condition"),
            EmptyIfBody => f.write_str("The `if` command is missing its body"),
            ElifMissingThen { .. } => f.write_str("The `elif` clause is missing the `then` clause"),
            EmptyElifCondition => f.write_str("The `elif` clause is missing its condition"),
            EmptyElifBody => f.write_str("The `elif` clause is missing its body"),
            EmptyElse => f.write_str("The `else` clause is missing its content"),
            UnclosedIf { .. } => f.write_str("The `if` command is missing its closing `fi`"),
            MissingCaseSubject => f.write_str("The subject is missing after `case`"),
            InvalidCaseSubject => f.write_str("The `case` command subject is not a valid word"),
            MissingIn { .. } => f.write_str("`in` is missing in the `case` command"),
            UnclosedPatternList => f.write_str("The pattern list is not properly closed by a `)`"),
            MissingPattern => f.write_str("A pattern is missing in the `case` command"),
            InvalidPattern => f.write_str("The pattern is not a valid word token"),
            EsacAsPattern => f.write_str("`esac` cannot be the first of a pattern list"),
            UnclosedCase { .. } => f.write_str("The `case` command is missing its closing `esac`"),
            UnmatchedParenthesis => f.write_str("`)` is missing after `(`"),
            MissingFunctionBody => f.write_str("The function body is missing"),
            InvalidFunctionBody => f.write_str("The function body must be a compound command"),
            MissingPipeline(and_or) => {
                write!(f, "A command is missing after `{}`", and_or)
            }
            DoubleNegation => f.write_str("`!` cannot be used twice in a row"),
            BangAfterBar => f.write_str("`!` cannot be used in the middle of a pipeline"),
            MissingCommandAfterBang => f.write_str("A command is missing after `!`"),
            MissingCommandAfterBar => f.write_str("A command is missing after `|`"),
        }
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

impl fmt::Display for ErrorCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCause::Io(e) => write!(f, "Error while reading commands: {}", e),
            ErrorCause::Syntax(e) => e.fmt(f),
        }
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
