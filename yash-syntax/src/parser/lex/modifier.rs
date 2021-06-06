// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Part of the lexer that parses suffix modifiers.

use super::core::Lexer;
use super::core::WordContext;
use super::core::WordLexer;
use crate::parser::core::Error;
use crate::parser::core::Result;
use crate::parser::core::SyntaxError;
use crate::syntax::Modifier;
use crate::syntax::Switch;
use crate::syntax::SwitchCondition;
use crate::syntax::SwitchType;
use crate::syntax::Trim;
use crate::syntax::TrimLength;
use crate::syntax::TrimSide;
use crate::syntax::Word;
use std::future::Future;
use std::pin::Pin;

impl Lexer {
    async fn invalid_modifier(&mut self) -> Result<Modifier> {
        let cause = SyntaxError::InvalidModifier.into();
        let location = self.location().await?.clone();
        Err(Error { cause, location })
    }

    async fn suffix_modifier_not_found(&mut self, colon: bool) -> Result<Modifier> {
        if colon {
            self.invalid_modifier().await
        } else {
            Ok(Modifier::None)
        }
    }

    /// Parses a [trim](Trim).
    ///
    /// This function blindly consumes the current character, which must be
    /// `symbol`.
    async fn trim(&mut self, colon: bool, symbol: char) -> Result<Modifier> {
        if colon {
            return self.invalid_modifier().await;
        }

        self.consume_char();
        let side = match symbol {
            '#' => TrimSide::Prefix,
            '%' => TrimSide::Suffix,
            _ => unreachable!(),
        };

        let length = if self.skip_if(|c| c == symbol).await? {
            TrimLength::Longest
        } else {
            TrimLength::Shortest
        };

        let mut lexer = WordLexer {
            lexer: self,
            context: WordContext::Word,
        };
        // Boxing needed for recursion
        let pattern =
            Box::pin(lexer.word(|c| c == '}')) as Pin<Box<dyn Future<Output = Result<Word>>>>;
        let mut pattern = pattern.await?;
        pattern.parse_tilde_front();

        Ok(Modifier::Trim(Trim {
            side,
            length,
            pattern,
        }))
    }
}

impl WordLexer<'_> {
    /// Parses a [switch](Switch), except the optional initial colon.
    ///
    /// This function blindly consumes the current character, which must be
    /// `symbol`.
    async fn switch(&mut self, colon: bool, symbol: char) -> Result<Modifier> {
        self.consume_char();
        let r#type = match symbol {
            '+' => SwitchType::Alter,
            '-' => SwitchType::Default,
            '=' => SwitchType::Assign,
            '?' => SwitchType::Error,
            _ => unreachable!(),
        };

        let condition = if colon {
            SwitchCondition::UnsetOrEmpty
        } else {
            SwitchCondition::Unset
        };

        // Boxing needed for recursion
        let word = Box::pin(self.word(|c| c == '}')) as Pin<Box<dyn Future<Output = Result<Word>>>>;
        let mut word = word.await?;
        match self.context {
            WordContext::Text => (),
            WordContext::Word => word.parse_tilde_front(),
        }

        Ok(Modifier::Switch(Switch {
            r#type,
            condition,
            word,
        }))
    }

    /// Parses a suffix modifier, i.e., a modifier other than the length prefix.
    ///
    /// If there is a [switch](Switch), [`self.context`](Self::context) affects
    /// how the word of the switch is parsed: If the context is `Word`, a tilde
    /// expansion is recognized at the beginning of the word and any character
    /// can be escaped by a backslash. If the context is `Text`, only `$`, `"`,
    /// `` ` ``, `\` and `}` can be escaped and single quotes are not recognized
    /// in the word.
    pub async fn suffix_modifier(&mut self) -> Result<Modifier> {
        let colon = self.skip_if(|c| c == ':').await?;

        if let Some(symbol) = self.peek_char().await? {
            match symbol {
                '+' | '-' | '=' | '?' => self.switch(colon, symbol).await,
                '#' | '%' => self.trim(colon, symbol).await,
                _ => self.suffix_modifier_not_found(colon).await,
            }
        } else {
            self.suffix_modifier_not_found(colon).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::core::ErrorCause;
    use crate::source::Source;
    use crate::syntax::Text;
    use crate::syntax::TextUnit;
    use crate::syntax::WordUnit;
    use futures::executor::block_on;

    #[test]
    fn lexer_suffix_modifier_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, "");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier());
        assert_eq!(result, Ok(Modifier::None));
    }

    #[test]
    fn lexer_suffix_modifier_none() {
        let mut lexer = Lexer::with_source(Source::Unknown, "}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier());
        assert_eq!(result, Ok(Modifier::None));

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_alter_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "+}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.units, []);
            assert_eq!(switch.word.location.line.value, "+}");
            assert_eq!(switch.word.location.column.get(), 2);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_alter_word() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"+a  z}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(
                switch.word.units,
                [
                    WordUnit::Unquoted(TextUnit::Literal('a')),
                    WordUnit::Unquoted(TextUnit::Literal(' ')),
                    WordUnit::Unquoted(TextUnit::Literal(' ')),
                    WordUnit::Unquoted(TextUnit::Literal('z')),
                ]
            );
            assert_eq!(switch.word.location.line.value, "+a  z}");
            assert_eq!(switch.word.location.column.get(), 2);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_alter_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, ":+}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.units, []);
            assert_eq!(switch.word.location.line.value, ":+}");
            assert_eq!(switch.word.location.column.get(), 3);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_default_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "-}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Default);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.units, []);
            assert_eq!(switch.word.location.line.value, "-}");
            assert_eq!(switch.word.location.column.get(), 2);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_default_word() {
        let mut lexer = Lexer::with_source(Source::Unknown, r":-cool}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Default);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(
                switch.word.units,
                [
                    WordUnit::Unquoted(TextUnit::Literal('c')),
                    WordUnit::Unquoted(TextUnit::Literal('o')),
                    WordUnit::Unquoted(TextUnit::Literal('o')),
                    WordUnit::Unquoted(TextUnit::Literal('l')),
                ]
            );
            assert_eq!(switch.word.location.line.value, ":-cool}");
            assert_eq!(switch.word.location.column.get(), 3);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_assign_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, ":=}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Assign);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.units, []);
            assert_eq!(switch.word.location.line.value, ":=}");
            assert_eq!(switch.word.location.column.get(), 3);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_assign_word() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"=Yes}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Assign);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(
                switch.word.units,
                [
                    WordUnit::Unquoted(TextUnit::Literal('Y')),
                    WordUnit::Unquoted(TextUnit::Literal('e')),
                    WordUnit::Unquoted(TextUnit::Literal('s')),
                ]
            );
            assert_eq!(switch.word.location.line.value, "=Yes}");
            assert_eq!(switch.word.location.column.get(), 2);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_error_empty() {
        let mut lexer = Lexer::with_source(Source::Unknown, "?}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Error);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.units, []);
            assert_eq!(switch.word.location.line.value, "?}");
            assert_eq!(switch.word.location.column.get(), 2);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_error_word() {
        let mut lexer = Lexer::with_source(Source::Unknown, r":?No}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.r#type, SwitchType::Error);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(
                switch.word.units,
                [
                    WordUnit::Unquoted(TextUnit::Literal('N')),
                    WordUnit::Unquoted(TextUnit::Literal('o')),
                ]
            );
            assert_eq!(switch.word.location.line.value, ":?No}");
            assert_eq!(switch.word.location.column.get(), 3);
        } else {
            panic!("Not a switch: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_tilde_expansion_in_switch_word_in_word_context() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"-~}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(switch.word.units, [WordUnit::Tilde("".to_string())]);
        } else {
            panic!("Not a switch: {:?}", result);
        }
    }

    #[test]
    fn lexer_suffix_modifier_tilde_expansion_in_switch_word_in_text_context() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"-~}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Switch(switch) = result {
            assert_eq!(
                switch.word.units,
                [WordUnit::Unquoted(TextUnit::Literal('~'))]
            );
        } else {
            panic!("Not a switch: {:?}", result);
        }
    }

    #[test]
    fn lexer_suffix_modifier_trim_shortest_prefix_in_word_context() {
        let mut lexer = Lexer::with_source(Source::Unknown, "#'*'}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Trim(trim) = result {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(trim.pattern.units, [WordUnit::SingleQuote("*".to_string())]);
            assert_eq!(trim.pattern.location.line.value, "#'*'}");
            assert_eq!(trim.pattern.location.column.get(), 2);
        } else {
            panic!("Not a trim: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_shortest_prefix_in_text_context() {
        let mut lexer = Lexer::with_source(Source::Unknown, "#'*'}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Trim(trim) = result {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(trim.pattern.units, [WordUnit::SingleQuote("*".to_string())]);
            assert_eq!(trim.pattern.location.line.value, "#'*'}");
            assert_eq!(trim.pattern.location.column.get(), 2);
        } else {
            panic!("Not a trim: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_longest_prefix() {
        let mut lexer = Lexer::with_source(Source::Unknown, r#"##"?"}"#);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Trim(trim) = result {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Longest);
            assert_eq!(trim.pattern.units.len(), 1, "{:?}", trim.pattern);
            if let WordUnit::DoubleQuote(Text(units)) = &trim.pattern.units[0] {
                assert_eq!(units[..], [TextUnit::Literal('?')]);
            } else {
                panic!("Not a double quote: {:?}", trim.pattern);
            }
            assert_eq!(trim.pattern.location.line.value, r#"##"?"}"#);
            assert_eq!(trim.pattern.location.column.get(), 3);
        } else {
            panic!("Not a trim: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_shortest_suffix() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"%\%}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Trim(trim) = result {
            assert_eq!(trim.side, TrimSide::Suffix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(
                trim.pattern.units,
                [WordUnit::Unquoted(TextUnit::Backslashed('%'))]
            );
            assert_eq!(trim.pattern.location.line.value, r"%\%}");
            assert_eq!(trim.pattern.location.column.get(), 2);
        } else {
            panic!("Not a trim: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_longest_suffix() {
        let mut lexer = Lexer::with_source(Source::Unknown, "%%%}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Trim(trim) = result {
            assert_eq!(trim.side, TrimSide::Suffix);
            assert_eq!(trim.length, TrimLength::Longest);
            assert_eq!(
                trim.pattern.units,
                [WordUnit::Unquoted(TextUnit::Literal('%'))]
            );
            assert_eq!(trim.pattern.location.line.value, "%%%}");
            assert_eq!(trim.pattern.location.column.get(), 3);
        } else {
            panic!("Not a trim: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_tilde_expansion_in_trim_word() {
        let mut lexer = Lexer::with_source(Source::Unknown, r"#~}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = block_on(lexer.suffix_modifier()).unwrap();
        if let Modifier::Trim(trim) = result {
            assert_eq!(trim.pattern.units, [WordUnit::Tilde("".to_string())]);
        } else {
            panic!("Not a trim: {:?}", result);
        }
    }

    #[test]
    fn lexer_suffix_modifier_orphan_colon_eof() {
        let mut lexer = Lexer::with_source(Source::Unknown, r":");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = block_on(lexer.suffix_modifier()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidModifier));
        assert_eq!(e.location.line.value, ":");
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn lexer_suffix_modifier_orphan_colon_followed_by_letter() {
        let mut lexer = Lexer::with_source(Source::Unknown, r":x}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = block_on(lexer.suffix_modifier()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidModifier));
        assert_eq!(e.location.line.value, ":x}");
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn lexer_suffix_modifier_orphan_colon_followed_by_symbol() {
        let mut lexer = Lexer::with_source(Source::Unknown, r":#}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = block_on(lexer.suffix_modifier()).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidModifier));
        assert_eq!(e.location.line.value, ":#}");
        assert_eq!(e.location.column.get(), 2);
    }
}
