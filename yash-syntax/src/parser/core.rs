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

use super::error::Error;
use super::error::SyntaxError;
use super::lex::Keyword;
use super::lex::Lexer;
use super::lex::Token;
use super::lex::TokenId::*;
use crate::alias::AliasSet;
use crate::parser::lex::is_blank;
use crate::syntax::HereDoc;
use crate::syntax::MaybeLiteral;
use std::rc::Rc;

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
/// # Parsing here-documents
///
/// Most intrinsic functions of `Parser` may return an AST containing `HereDoc`s
/// with empty content. The parser creates the `HereDoc` instance when it finds
/// a here-document operator, but it has not read its content at that time. When
/// finding a newline token, the parser reads the content and fills it into the
/// `HereDoc` instance.
///
/// Unless you are interested in parsing a specific syntactic construct that is
/// only part of source code, you will want to use a function that returns a
/// complete result filled with proper here-document contents if any.
/// Then the [`command_line`](Self::command_line) function is for you.
/// See also the [module documentation](super).
#[derive(Debug)]
pub struct Parser<'a, 'b> {
    /// Lexer that provides tokens.
    lexer: &'a mut Lexer<'b>,

    /// Aliases that are used while parsing.
    aliases: &'a AliasSet,

    /// Token to parse next.
    ///
    /// This value is an option of a result. It is `None` when the next token is not yet parsed by
    /// the lexer. It is `Some(Err(_))` if the lexer has failed.
    token: Option<Result<Token>>,

    /// Here-documents without contents.
    ///
    /// The here-document is added to this list when the parser finds a
    /// here-document operator. After consuming the next newline token, the
    /// parser reads and fills the contents, then clears this list.
    unread_here_docs: Vec<Rc<HereDoc>>,
}

impl<'a, 'b> Parser<'a, 'b> {
    /// Creates a new parser based on the given lexer and alias set.
    pub fn new(lexer: &'a mut Lexer<'b>, aliases: &'a AliasSet) -> Parser<'a, 'b> {
        Parser {
            lexer,
            aliases,
            token: None,
            unread_here_docs: vec![],
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
                    if !token.word.location.code.source.is_alias_for(&name) {
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
    ///
    /// The remembered here-document's content will be parsed when
    /// [`here_doc_contents`](Self::here_doc_contents) is called later.
    pub fn memorize_unread_here_doc(&mut self, here_doc: Rc<HereDoc>) {
        self.unread_here_docs.push(here_doc)
    }

    /// Reads here-document contents that matches the remembered list of
    /// here-document operators.
    ///
    /// This function reads here-document contents corresponding to
    /// here-document operators that have been saved with
    /// [`memorize_unread_here_doc`](Self::memorize_unread_here_doc).
    /// The results are inserted to the `content` field of the `HereDoc`
    /// instances.
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

        for here_doc in self.unread_here_docs.drain(..) {
            self.lexer.here_doc_content(&here_doc).await?;
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
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::AliasSet;
    use crate::alias::HashEntry;
    use crate::source::Location;
    use crate::source::Source;
    use crate::syntax::Text;
    use futures_executor::block_on;
    use std::cell::RefCell;

    #[test]
    fn parser_take_token_manual_successful_substitution() {
        block_on(async {
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

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
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_manual(false).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");
        });
    }

    #[test]
    fn parser_take_token_manual_not_literal() {
        block_on(async {
            let mut lexer = Lexer::from_memory(r"\X", Source::Unknown);
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
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), r"\X");
        });
    }

    #[test]
    fn parser_take_token_manual_operator() {
        block_on(async {
            let mut lexer = Lexer::from_memory(";", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                ";".to_string(),
                "x".to_string(),
                false,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.id, Operator(super::super::lex::Operator::Semicolon));
            assert_eq!(token.word.to_string_if_literal().unwrap(), ";");
        })
    }

    #[test]
    fn parser_take_token_manual_no_match() {
        block_on(async {
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_manual(true).await.unwrap().unwrap();
            assert_eq!(token.to_string(), "X");
        });
    }

    #[test]
    fn parser_take_token_manual_recursive_substitution() {
        block_on(async {
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
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
            let mut parser = Parser::new(&mut lexer, &aliases);

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
            let mut lexer = Lexer::from_memory("X\tY", Source::Unknown);
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
            let mut parser = Parser::new(&mut lexer, &aliases);

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
            let mut lexer = Lexer::from_memory("X\tY", Source::Unknown);
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
            let mut parser = Parser::new(&mut lexer, &aliases);

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
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

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
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "X".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_auto(&[]).await.unwrap();
            assert_eq!(token.to_string(), "x");
        })
    }

    #[test]
    fn parser_take_token_auto_keyword_matched() {
        block_on(async {
            let mut lexer = Lexer::from_memory("if", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "if".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_auto(&[Keyword::If]).await.unwrap();
            assert_eq!(token.to_string(), "if");
        })
    }

    #[test]
    fn parser_take_token_auto_keyword_unmatched() {
        block_on(async {
            let mut lexer = Lexer::from_memory("if", Source::Unknown);
            let mut aliases = AliasSet::new();
            aliases.insert(HashEntry::new(
                "if".to_string(),
                "x".to_string(),
                true,
                Location::dummy("?"),
            ));
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_auto(&[]).await.unwrap();
            assert_eq!(token.to_string(), "x");
        })
    }

    #[test]
    fn parser_take_token_auto_alias_substitution_to_keyword_matched() {
        block_on(async {
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
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
            let mut parser = Parser::new(&mut lexer, &aliases);

            let token = parser.take_token_auto(&[Keyword::If]).await.unwrap();
            assert_eq!(token.to_string(), "if");
        })
    }

    #[test]
    fn parser_has_blank_true() {
        block_on(async {
            let mut lexer = Lexer::from_memory(" ", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(true));
        });
    }

    #[test]
    fn parser_has_blank_false() {
        block_on(async {
            let mut lexer = Lexer::from_memory("(", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(false));
        });
    }

    #[test]
    fn parser_has_blank_eof() {
        block_on(async {
            let mut lexer = Lexer::from_memory("", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(false));
        });
    }

    #[test]
    fn parser_has_blank_true_with_line_continuations() {
        block_on(async {
            let mut lexer = Lexer::from_memory("\\\n\\\n ", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(true));
        });
    }

    #[test]
    fn parser_has_blank_false_with_line_continuations() {
        block_on(async {
            let mut lexer = Lexer::from_memory("\\\n\\\n\\\n(", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let result = parser.has_blank().await;
            assert_eq!(result, Ok(false));
        });
    }

    #[test]
    #[should_panic(expected = "There should be no pending token")]
    fn parser_has_blank_with_pending_token() {
        block_on(async {
            let mut lexer = Lexer::from_memory("foo", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            parser.peek_token().await.unwrap();
            let _ = parser.has_blank().await;
        });
    }

    #[test]
    fn parser_reading_no_here_doc_contents() {
        block_on(async {
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            parser.here_doc_contents().await.unwrap();

            let location = lexer.location().await.unwrap();
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(location.index, 0);
        })
    }

    #[test]
    fn parser_reading_one_here_doc_content() {
        let delimiter = "END".parse().unwrap();

        block_on(async {
            let mut lexer = Lexer::from_memory("END\nX", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let remove_tabs = false;
            let here_doc = Rc::new(HereDoc {
                delimiter,
                remove_tabs,
                content: RefCell::new(Text(Vec::new())),
            });
            parser.memorize_unread_here_doc(Rc::clone(&here_doc));
            parser.here_doc_contents().await.unwrap();
            assert_eq!(here_doc.delimiter.to_string(), "END");
            assert_eq!(here_doc.remove_tabs, remove_tabs);
            assert_eq!(here_doc.content.borrow().0, []);

            let location = lexer.location().await.unwrap();
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(location.index, 4);
        })
    }

    #[test]
    fn parser_reading_many_here_doc_contents() {
        let delimiter1 = "ONE".parse().unwrap();
        let delimiter2 = "TWO".parse().unwrap();
        let delimiter3 = "THREE".parse().unwrap();

        block_on(async {
            let mut lexer = Lexer::from_memory("1\nONE\nTWO\n3\nTHREE\nX", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let here_doc1 = Rc::new(HereDoc {
                delimiter: delimiter1,
                remove_tabs: false,
                content: RefCell::new(Text(Vec::new())),
            });
            parser.memorize_unread_here_doc(Rc::clone(&here_doc1));
            let here_doc2 = Rc::new(HereDoc {
                delimiter: delimiter2,
                remove_tabs: true,
                content: RefCell::new(Text(Vec::new())),
            });
            parser.memorize_unread_here_doc(Rc::clone(&here_doc2));
            let here_doc3 = Rc::new(HereDoc {
                delimiter: delimiter3,
                remove_tabs: false,
                content: RefCell::new(Text(Vec::new())),
            });
            parser.memorize_unread_here_doc(Rc::clone(&here_doc3));
            parser.here_doc_contents().await.unwrap();
            assert_eq!(here_doc1.delimiter.to_string(), "ONE");
            assert_eq!(here_doc1.remove_tabs, false);
            assert_eq!(here_doc1.content.borrow().to_string(), "1\n");
            assert_eq!(here_doc2.delimiter.to_string(), "TWO");
            assert_eq!(here_doc2.remove_tabs, true);
            assert_eq!(here_doc2.content.borrow().to_string(), "");
            assert_eq!(here_doc3.delimiter.to_string(), "THREE");
            assert_eq!(here_doc3.remove_tabs, false);
            assert_eq!(here_doc3.content.borrow().to_string(), "3\n");
        })
    }

    #[test]
    fn parser_reading_here_doc_contents_twice() {
        let delimiter1 = "ONE".parse().unwrap();
        let delimiter2 = "TWO".parse().unwrap();

        block_on(async {
            let mut lexer = Lexer::from_memory("1\nONE\n2\nTWO\n", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            let here_doc1 = Rc::new(HereDoc {
                delimiter: delimiter1,
                remove_tabs: false,
                content: RefCell::new(Text(Vec::new())),
            });
            parser.memorize_unread_here_doc(Rc::clone(&here_doc1));
            parser.here_doc_contents().await.unwrap();
            let here_doc2 = Rc::new(HereDoc {
                delimiter: delimiter2,
                remove_tabs: true,
                content: RefCell::new(Text(Vec::new())),
            });
            parser.memorize_unread_here_doc(Rc::clone(&here_doc2));
            parser.here_doc_contents().await.unwrap();
            assert_eq!(here_doc1.delimiter.to_string(), "ONE");
            assert_eq!(here_doc1.remove_tabs, false);
            assert_eq!(here_doc1.content.borrow().to_string(), "1\n");
            assert_eq!(here_doc2.delimiter.to_string(), "TWO");
            assert_eq!(here_doc2.remove_tabs, true);
            assert_eq!(here_doc2.content.borrow().to_string(), "2\n");
        })
    }

    #[test]
    #[should_panic(expected = "No token must be peeked before reading here-doc contents")]
    fn parser_here_doc_contents_must_be_called_without_pending_token() {
        block_on(async {
            let mut lexer = Lexer::from_memory("X", Source::Unknown);
            let aliases = AliasSet::new();
            let mut parser = Parser::new(&mut lexer, &aliases);
            parser.peek_token().await.unwrap();
            parser.here_doc_contents().await.unwrap();
        })
    }
}
