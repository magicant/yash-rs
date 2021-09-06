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

//! Fundamentals for implementing the parser.
//!
//! This module includes common types that are used as building blocks for constructing the syntax
//! parser.

use super::lex::Keyword;
use super::lex::Lexer;
use super::lex::PartialHereDoc;
use super::lex::Token;
use super::lex::TokenId::*;
use crate::alias::AliasSet;
use crate::parser::lex::is_blank;
use crate::source::Location;
use crate::syntax::AndOr;
use crate::syntax::HereDoc;
use crate::syntax::MaybeLiteral;
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

/// Entire result of parsing.
pub type Result<T> = std::result::Result<T, Error>;

/// Modifier that makes a result of parsing optional in order to trigger the parser to restart
/// parsing after alias substitution.
///
/// `Rec` stands for "recursion", as it is used to make the parser work recursively.
///
/// This enum type has two variants: `AliasSubstituted` and `Parsed`. The former contains no
/// meaningful value and is returned from a parsing function that has performed alias substitution
/// without consuming any tokens. In this case, the caller of the parsing function must inspect the
/// new source code produced by the substitution so that the syntax is correctly recognized in the
/// new code.
///
/// Assume we have an alias definition `untrue='! true'`, for example. When the word `untrue` is
/// recognized as an alias name during parse of a simple command, the simple command parser
/// function must stop parsing and return `AliasSubstituted`. This allows the caller, the pipeline
/// parser function, to recognize the `!` reserved word token as negation.
///
/// When a parser function successfully parses something, it returns the result in the `Parsed`
/// variant. The caller then continues the remaining parse.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Rec<T> {
    /// Result of alias substitution.
    AliasSubstituted,
    /// Successful parse result.
    Parsed(T),
}

impl<T> Rec<T> {
    /// Tests if `self` is `AliasSubstituted`.
    pub fn is_alias_substituted(&self) -> bool {
        match self {
            Rec::AliasSubstituted => true,
            Rec::Parsed(_) => false,
        }
    }

    /// Extracts the result of successful parsing.
    ///
    /// # Panics
    ///
    /// If `self` is `AliasSubstituted`.
    pub fn unwrap(self) -> T {
        match self {
            Rec::AliasSubstituted => panic!("Rec::AliasSubstituted cannot be unwrapped"),
            Rec::Parsed(v) => v,
        }
    }

    /// Transforms the result value in `self`.
    pub fn map<U, F>(self, f: F) -> Result<Rec<U>>
    where
        F: FnOnce(T) -> Result<U>,
    {
        match self {
            Rec::AliasSubstituted => Ok(Rec::AliasSubstituted),
            Rec::Parsed(t) => Ok(Rec::Parsed(f(t)?)),
        }
    }
}

/// The shell syntax parser.
///
/// This `struct` contains a set of data used in syntax parsing.
///
/// Because of the nature of here-document contents that cannot be parsed by a
/// standard recursive descent parser, most intrinsic functions of `Parser`
/// return an AST with [`MissingHereDoc`](super::MissingHereDoc) filled in place
/// of a proper here-document data. When the parser parses a here-document
/// content, the content is accumulated in the parser so that it can be merged
/// into the main AST in the final step of parsing.
///
/// Unless you are interested in parsing a specific syntactic construct that is
/// only part of source code, you will want to use a function that returns a
/// complete result filled with proper here-document contents if any.
/// Then the [`command_line`](Self::command_line) function is for you.
/// See also the [module documentation](super).
#[derive(Debug)]
pub struct Parser<'l> {
    /// Lexer that provides tokens.
    lexer: &'l mut Lexer,

    /// Aliases that are used while parsing.
    aliases: Rc<AliasSet>,

    /// Token to parse next.
    ///
    /// This value is an option of a result. It is `None` when the next token is not yet parsed by
    /// the lexer. It is `Some(Err(_))` if the lexer has failed.
    token: Option<Result<Token>>,

    /// Here-documents without contents.
    ///
    /// The contents must be read just after a next newline token is parsed.
    unread_here_docs: Vec<PartialHereDoc>,

    /// Here-documents with contents.
    ///
    /// After here-document contents have been read, the results are saved in this vector until
    /// they are merged into the whose parse result.
    read_here_docs: Vec<HereDoc>,
}

impl Parser<'_> {
    /// Creates a new parser based on the given lexer.
    ///
    /// The parser created by this function does not perform alias substitution. To do it, pass an
    /// alias set to [`with_aliases`](Parser::with_aliases).
    pub fn new(lexer: &mut Lexer) -> Parser {
        Self::with_aliases(lexer, Rc::new(AliasSet::new()))
    }

    /// Creates a new parser based on the given lexer and alias set.
    pub fn with_aliases(lexer: &mut Lexer, aliases: Rc<AliasSet>) -> Parser {
        Parser {
            lexer,
            aliases,
            token: None,
            unread_here_docs: vec![],
            read_here_docs: vec![],
        }
    }

    /// Reads a next token if the current token is `None`.
    async fn require_token(&mut self) {
        if self.token.is_none() {
            self.token = Some(if let Err(e) = self.lexer.skip_blanks_and_comment().await {
                Err(e)
            } else {
                self.lexer.token().await
            });
        }
    }

    /// Returns a reference to the current token.
    ///
    /// If the current token is not yet read from the underlying lexer, it is read.
    pub async fn peek_token(&mut self) -> Result<&Token> {
        self.require_token().await;
        self.token.as_ref().unwrap().as_ref().map_err(|e| e.clone())
    }

    /// Consumes the current token without performing alias substitution.
    ///
    /// If the current token is not yet read from the underlying lexer, it is read.
    ///
    /// This function does not perform alias substitution and therefore should be
    /// used only in context where no alias substitution is expected. Otherwise,
    /// you should use [`take_token_manual`](Self::take_token_manual) or
    /// [`take_token_auto`](Self::take_token_auto) instead.
    pub async fn take_token_raw(&mut self) -> Result<Token> {
        self.require_token().await;
        self.token.take().unwrap()
    }

    /// Performs alias substitution on a token that has just been
    /// [taken](Self::take_token_raw).
    fn substitute_alias(&mut self, token: Token, is_command_name: bool) -> Rec<Token> {
        // TODO Only POSIXly-valid alias name should be recognized in POSIXly-correct mode.
        if !self.aliases.is_empty() {
            if let Token(_) = token.id {
                if let Some(name) = token.word.to_string_if_literal() {
                    if !token.word.location.line.source.is_alias_for(&name) {
                        if let Some(alias) = self.aliases.get(&name as &str) {
                            if is_command_name
                                || alias.0.global
                                || self.lexer.is_after_blank_ending_alias(token.index)
                            {
                                self.lexer.substitute_alias(token.index, &alias.0);
                                return Rec::AliasSubstituted;
                            }
                        }
                    }
                }
            }
        }

        Rec::Parsed(token)
    }

    /// Consumes the current token after performing applicable alias substitution.
    ///
    /// If the current token is not yet read from the underlying lexer, it is read.
    ///
    /// This function checks if the token is the name of an alias. If it is,
    /// alias substitution is performed on the token and the result is
    /// `Ok(AliasSubstituted)`. Otherwise, the token is consumed and returned.
    ///
    /// Alias substitution is performed only if at least one of the following is
    /// true:
    ///
    /// - The token is the first command word in a simple command, that is, it is
    ///   the word for the command name. (This condition should be specified by the
    ///   `is_command_name` parameter.)
    /// - The token comes just after the replacement string of another alias
    ///   substitution that ends with a blank character.
    /// - The token names a global alias.
    ///
    /// However, alias substitution should _not_ be performed on a reserved word
    /// in any case. It is your responsibility to check the token type and not to
    /// call this function on a reserved word. That is why this function is named
    /// `manual`. To consume a reserved word without performing alias
    /// substitution, you should call [`take_token_raw`](Self::take_token_raw) or
    /// [`take_token_auto`](Self::take_token_auto).
    pub async fn take_token_manual(&mut self, is_command_name: bool) -> Result<Rec<Token>> {
        let token = self.take_token_raw().await?;
        Ok(self.substitute_alias(token, is_command_name))
    }

    /// Consumes the current token after performing applicable alias substitution.
    ///
    /// This function performs alias substitution unless the result is one of the
    /// reserved words specified in the argument.
    ///
    /// Alias substitution is performed repeatedly until a non-alias token is
    /// found. That is why this function is named `auto`. This function should be
    /// used only in contexts where no backtrack is needed after alias
    /// substitution. If you need to backtrack or want to know whether alias
    /// substitution was performed or not, you should use
    /// [`Self::take_token_manual`](Self::take_token_manual), which performs
    /// alias substitution at most once and returns `Rec`.
    pub async fn take_token_auto(&mut self, keywords: &[Keyword]) -> Result<Token> {
        loop {
            let token = self.take_token_raw().await?;
            if let Token(Some(keyword)) = token.id {
                if keywords.contains(&keyword) {
                    return Ok(token);
                }
            }
            if let Rec::Parsed(token) = self.substitute_alias(token, false) {
                return Ok(token);
            }
        }
    }

    /// Tests if there is a blank before the next token.
    ///
    /// This function can be called to tell whether the previous and next tokens
    /// are separated by a blank or they are adjacent.
    ///
    /// This function must be called after the previous token has been taken (by
    /// one of [`take_token_raw`](Self::take_token_raw),
    /// [`take_token_manual`](Self::take_token_manual) and
    /// [`take_token_auto`](Self::take_token_auto)) and before the next token is
    /// [peeked](Self::peek_token). Otherwise, this function would panic.
    ///
    /// # Panics
    ///
    /// If the previous token has not been taken or the next token has been
    /// peeked.
    pub async fn has_blank(&mut self) -> Result<bool> {
        assert!(self.token.is_none(), "There should be no pending token");
        let c = self.lexer.peek_char().await?;
        Ok(c.map_or(false, is_blank))
    }

    /// Remembers the given partial here-document for later parsing of its content.
    pub fn memorize_unread_here_doc(&mut self, here_doc: PartialHereDoc) {
        self.unread_here_docs.push(here_doc)
    }

    /// Reads here-document contents that matches the remembered list of partial here-documents.
    ///
    /// The results are accumulated in the internal list of (non-partial) here-documents.
    ///
    /// This function must be called just after a newline token has been taken
    /// (either [manual](Self::take_token_manual) or
    /// [auto](Self::take_token_auto)). If there is a pending token that has been
    /// peeked but not yet taken, this function will panic!
    pub async fn here_doc_contents(&mut self) -> Result<()> {
        assert!(
            self.token.is_none(),
            "No token must be peeked before reading here-doc contents"
        );

        self.read_here_docs
            .reserve_exact(self.unread_here_docs.len());

        for here_doc in self.unread_here_docs.drain(..) {
            self.read_here_docs
                .push(self.lexer.here_doc_content(here_doc).await?);
        }

        Ok(())
    }

    /// Ensures that there is no pending partial here-document.
    ///
    /// If there is any, this function returns a `MissingHereDocContent` error.
    pub fn ensure_no_unread_here_doc(&self) -> Result<()> {
        match self.unread_here_docs.first() {
            None => Ok(()),
            Some(here_doc) => Err(Error {
                cause: SyntaxError::MissingHereDocContent.into(),
                location: here_doc.delimiter.location.clone(),
            }),
        }
    }

    /// Returns a list of here-documents with contents that have been read.
    pub fn take_read_here_docs(&mut self) -> Vec<HereDoc> {
        std::mem::take(&mut self.read_here_docs)
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::AliasSet;
    use crate::alias::HashEntry;
    use crate::source::Line;
    use crate::source::Source;
    use futures_executor::block_on;
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

    #[test]
    fn parser_take_token_manual_successful_substitution() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "x");
        });
    }

    #[test]
    fn parser_take_token_manual_not_command_name() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(false).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");
        });
    }

    #[test]
    fn parser_take_token_manual_not_literal() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, r"\X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            aliases.insert(HashEntry::new(
                r"\X".to_string(),
                "quoted".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), r"\X");
        });
    }

    #[test]
    fn parser_take_token_manual_operator() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, ";");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                ";".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.id, Operator(super::super::lex::Operator::Semicolon));
            assert_eq!(token.word.to_string_if_literal().unwrap(), ";");
        })
    }

    #[test]
    fn parser_take_token_manual_no_match() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let aliases = AliasSet::new();
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");
        });
    }

    #[test]
    fn parser_take_token_manual_recursive_substitution() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "Y x".to_string(),
                false,
                Location::dummy("?"),
            ));
            aliases.insert(HashEntry::new(
                "Y".to_string(),
                "X y".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(true).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "y");

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "x");
        });
    }

    #[test]
    fn parser_take_token_manual_after_blank_ending_substitution() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X\tY");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                " X ".to_string(),
                false,
                Location::dummy("?"),
            ));
            aliases.insert(HashEntry::new(
                "Y".to_string(),
                "y".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");

            let token = parser.take_token_manual(false).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(false).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "y");
        });
    }

    #[test]
    fn parser_take_token_manual_not_after_blank_ending_substitution() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X\tY");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                " X".to_string(),
                false,
                Location::dummy("?"),
            ));
            aliases.insert(HashEntry::new(
                "Y".to_string(),
                "y".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(true).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");

            let token = parser.take_token_manual(false).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "Y");
        });
    }

    #[test]
    fn parser_take_token_manual_global() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_manual(false).await.unwrap();
            assert!(
                token.is_alias_substituted(),
                "{:?} should be AliasSubstituted",
                &token
            );

            let token = parser.take_token_manual(false).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "x");
        });
    }

    #[test]
    fn parser_take_token_auto_non_keyword() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_auto(&[]).await.unwrap();
            assert_eq!(token.to_string(), "x");
        })
    }

    #[test]
    fn parser_take_token_auto_keyword_matched() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "if");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "if".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_auto(&[Keyword::If]).await.unwrap();
            assert_eq!(token.to_string(), "if");
        })
    }

    #[test]
    fn parser_take_token_auto_keyword_unmatched() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "if");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "if".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_auto(&[]).await.unwrap();
            assert_eq!(token.to_string(), "x");
        })
    }

    #[test]
    fn parser_take_token_auto_alias_substitution_to_keyword_matched() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "if".to_string(),
                true,
                Location::dummy("?"),
            ));
            aliases.insert(HashEntry::new(
                "if".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::with_aliases(&mut lexer, Rc::new(aliases));

            let token = parser.take_token_auto(&[Keyword::If]).await.unwrap();
            assert_eq!(token.to_string(), "if");
        })
    }

    #[test]
    fn parser_has_blank_true() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, " ");
            let mut parser = Parser::new(&mut lexer);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(true));
        });
    }

    #[test]
    fn parser_has_blank_false() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "(");
            let mut parser = Parser::new(&mut lexer);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(false));
        });
    }

    #[test]
    fn parser_has_blank_eof() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "");
            let mut parser = Parser::new(&mut lexer);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(false));
        });
    }

    #[test]
    fn parser_has_blank_true_with_line_continuations() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "\\\n\\\n ");
            let mut parser = Parser::new(&mut lexer);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(true));
        });
    }

    #[test]
    fn parser_has_blank_false_with_line_continuations() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "\\\n\\\n\\\n(");
            let mut parser = Parser::new(&mut lexer);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(false));
        });
    }

    #[test]
    #[should_panic(expected = "There should be no pending token")]
    fn parser_has_blank_with_pending_token() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "foo");
            let mut parser = Parser::new(&mut lexer);
            parser.peek_token().await.unwrap();
            let _ = parser.has_blank().await;
        });
    }

    #[test]
    fn parser_reading_no_here_doc_contents() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut parser = Parser::new(&mut lexer);
            parser.here_doc_contents().await.unwrap();
            assert!(parser.take_read_here_docs().is_empty());

            let location = lexer.location().await.unwrap();
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.column.get(), 1);
        })
    }

    #[test]
    fn parser_reading_one_here_doc_content() {
        let delimiter = "END".parse().unwrap();

        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "END\nX");
            let mut parser = Parser::new(&mut lexer);
            let remove_tabs = false;
            parser.memorize_unread_here_doc(PartialHereDoc {
                delimiter,
                remove_tabs,
            });
            parser.here_doc_contents().await.unwrap();
            let here_docs = parser.take_read_here_docs();
            assert_eq!(here_docs.len(), 1);
            assert_eq!(here_docs[0].delimiter.to_string(), "END");
            assert_eq!(here_docs[0].remove_tabs, remove_tabs);
            assert_eq!(here_docs[0].content.0, []);

            assert!(parser.take_read_here_docs().is_empty());

            let location = lexer.location().await.unwrap();
            assert_eq!(location.line.number.get(), 2);
            assert_eq!(location.column.get(), 1);
        })
    }

    #[test]
    fn parser_reading_many_here_doc_contents() {
        let delimiter1 = "ONE".parse().unwrap();
        let delimiter2 = "TWO".parse().unwrap();
        let delimiter3 = "THREE".parse().unwrap();

        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "1\nONE\nTWO\n3\nTHREE\nX");
            let mut parser = Parser::new(&mut lexer);
            parser.memorize_unread_here_doc(PartialHereDoc {
                delimiter: delimiter1,
                remove_tabs: false,
            });
            parser.memorize_unread_here_doc(PartialHereDoc {
                delimiter: delimiter2,
                remove_tabs: true,
            });
            parser.memorize_unread_here_doc(PartialHereDoc {
                delimiter: delimiter3,
                remove_tabs: false,
            });
            parser.here_doc_contents().await.unwrap();
            let here_docs = parser.take_read_here_docs();
            assert_eq!(here_docs.len(), 3);
            assert_eq!(here_docs[0].delimiter.to_string(), "ONE");
            assert_eq!(here_docs[0].remove_tabs, false);
            assert_eq!(here_docs[0].content.to_string(), "1\n");
            assert_eq!(here_docs[1].delimiter.to_string(), "TWO");
            assert_eq!(here_docs[1].remove_tabs, true);
            assert_eq!(here_docs[1].content.to_string(), "");
            assert_eq!(here_docs[2].delimiter.to_string(), "THREE");
            assert_eq!(here_docs[2].remove_tabs, false);
            assert_eq!(here_docs[2].content.to_string(), "3\n");
        })
    }

    #[test]
    fn parser_reading_here_doc_contents_twice() {
        let delimiter1 = "ONE".parse().unwrap();
        let delimiter2 = "TWO".parse().unwrap();

        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "1\nONE\n2\nTWO\n");
            let mut parser = Parser::new(&mut lexer);
            parser.memorize_unread_here_doc(PartialHereDoc {
                delimiter: delimiter1,
                remove_tabs: false,
            });
            parser.here_doc_contents().await.unwrap();
            let here_docs = parser.take_read_here_docs();
            assert_eq!(here_docs.len(), 1);
            assert_eq!(here_docs[0].delimiter.to_string(), "ONE");
            assert_eq!(here_docs[0].remove_tabs, false);
            assert_eq!(here_docs[0].content.to_string(), "1\n");

            parser.memorize_unread_here_doc(PartialHereDoc {
                delimiter: delimiter2,
                remove_tabs: true,
            });
            parser.here_doc_contents().await.unwrap();
            let here_docs = parser.take_read_here_docs();
            assert_eq!(here_docs.len(), 1);
            assert_eq!(here_docs[0].delimiter.to_string(), "TWO");
            assert_eq!(here_docs[0].remove_tabs, true);
            assert_eq!(here_docs[0].content.to_string(), "2\n");
        })
    }

    #[test]
    #[should_panic(expected = "No token must be peeked before reading here-doc contents")]
    fn parser_here_doc_contents_must_be_called_without_pending_token() {
        block_on(async {
            let mut lexer = Lexer::with_source(Source::Unknown, "X");
            let mut parser = Parser::new(&mut lexer);
            parser.peek_token().await.unwrap();
            parser.here_doc_contents().await.unwrap();
        })
    }
}
