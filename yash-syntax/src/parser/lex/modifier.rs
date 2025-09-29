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

//! Part of the lexer that parses suffix modifiers

use super::core::Lexer;
use super::core::WordContext;
use super::core::WordLexer;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::syntax::Modifier;
use crate::syntax::Switch;
use crate::syntax::SwitchAction;
use crate::syntax::SwitchCondition;
use crate::syntax::Trim;
use crate::syntax::TrimLength;
use crate::syntax::TrimSide;

impl Lexer<'_> {
    /// Returns an invalid modifier error.
    ///
    /// The `start_index` must be the index of the first character of the modifier.
    fn invalid_modifier(&mut self, start_index: usize) -> Result<Modifier> {
        let cause = SyntaxError::InvalidModifier.into();
        let location = self.location_range(start_index..self.index());
        Err(Error { cause, location })
    }

    fn suffix_modifier_not_found(&mut self, start_index: usize, colon: bool) -> Result<Modifier> {
        if colon {
            self.invalid_modifier(start_index)
        } else {
            Ok(Modifier::None)
        }
    }

    /// Parses a [trim](Trim).
    ///
    /// This function blindly consumes the current character, which must be
    /// `symbol`.
    async fn trim(&mut self, start_index: usize, colon: bool, symbol: char) -> Result<Modifier> {
        self.consume_char();
        if colon {
            return self.invalid_modifier(start_index);
        }

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
        let mut pattern = Box::pin(lexer.word(|c| c == '}')).await?;
        pattern.parse_tilde_front();

        Ok(Modifier::Trim(Trim {
            side,
            length,
            pattern,
        }))
    }
}

impl WordLexer<'_, '_> {
    /// Parses a [switch](Switch), except the optional initial colon.
    ///
    /// This function blindly consumes the current character, which must be
    /// `symbol`.
    async fn switch(&mut self, colon: bool, symbol: char) -> Result<Modifier> {
        self.consume_char();
        let action = match symbol {
            '+' => SwitchAction::Alter,
            '-' => SwitchAction::Default,
            '=' => SwitchAction::Assign,
            '?' => SwitchAction::Error,
            _ => unreachable!(),
        };

        let condition = if colon {
            SwitchCondition::UnsetOrEmpty
        } else {
            SwitchCondition::Unset
        };

        // Boxing needed for recursion
        let mut word = Box::pin(self.word(|c| c == '}')).await?;
        match self.context {
            WordContext::Text => (),
            WordContext::Word => word.parse_tilde_front(),
        }

        Ok(Modifier::Switch(Switch {
            action,
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
        let start_index = self.index();
        let colon = self.skip_if(|c| c == ':').await?;

        if let Some(symbol) = self.peek_char().await? {
            match symbol {
                '+' | '-' | '=' | '?' => self.switch(colon, symbol).await,
                '#' | '%' => self.trim(start_index, colon, symbol).await,
                _ => self.suffix_modifier_not_found(start_index, colon),
            }
        } else {
            self.suffix_modifier_not_found(start_index, colon)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::syntax::Text;
    use crate::syntax::TextUnit;
    use crate::syntax::WordUnit;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn lexer_suffix_modifier_eof() {
        let mut lexer = Lexer::with_code("");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap();
        assert_eq!(result, Ok(Modifier::None));
    }

    #[test]
    fn lexer_suffix_modifier_none() {
        let mut lexer = Lexer::with_code("}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap();
        assert_eq!(result, Ok(Modifier::None));

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_alter_empty() {
        let mut lexer = Lexer::with_code("+}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.units, []);
            assert_eq!(*switch.word.location.code.value.borrow(), "+}");
            assert_eq!(switch.word.location.range, 1..1);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_alter_word() {
        let mut lexer = Lexer::with_code(r"+a  z}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Alter);
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
            assert_eq!(*switch.word.location.code.value.borrow(), "+a  z}");
            assert_eq!(switch.word.location.range, 1..5);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_alter_empty() {
        let mut lexer = Lexer::with_code(":+}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Alter);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.units, []);
            assert_eq!(*switch.word.location.code.value.borrow(), ":+}");
            assert_eq!(switch.word.location.range, 2..2);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_default_empty() {
        let mut lexer = Lexer::with_code("-}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Default);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.units, []);
            assert_eq!(*switch.word.location.code.value.borrow(), "-}");
            assert_eq!(switch.word.location.range, 1..1);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_default_word() {
        let mut lexer = Lexer::with_code(r":-cool}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Default);
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
            assert_eq!(*switch.word.location.code.value.borrow(), ":-cool}");
            assert_eq!(switch.word.location.range, 2..6);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_assign_empty() {
        let mut lexer = Lexer::with_code(":=}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Assign);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.units, []);
            assert_eq!(*switch.word.location.code.value.borrow(), ":=}");
            assert_eq!(switch.word.location.range, 2..2);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_assign_word() {
        let mut lexer = Lexer::with_code(r"=Yes}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Assign);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(
                switch.word.units,
                [
                    WordUnit::Unquoted(TextUnit::Literal('Y')),
                    WordUnit::Unquoted(TextUnit::Literal('e')),
                    WordUnit::Unquoted(TextUnit::Literal('s')),
                ]
            );
            assert_eq!(*switch.word.location.code.value.borrow(), "=Yes}");
            assert_eq!(switch.word.location.range, 1..4);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_error_empty() {
        let mut lexer = Lexer::with_code("?}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Error);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.units, []);
            assert_eq!(*switch.word.location.code.value.borrow(), "?}");
            assert_eq!(switch.word.location.range, 1..1);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_colon_error_word() {
        let mut lexer = Lexer::with_code(r":?No}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(switch.action, SwitchAction::Error);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(
                switch.word.units,
                [
                    WordUnit::Unquoted(TextUnit::Literal('N')),
                    WordUnit::Unquoted(TextUnit::Literal('o')),
                ]
            );
            assert_eq!(*switch.word.location.code.value.borrow(), ":?No}");
            assert_eq!(switch.word.location.range, 2..4);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_tilde_expansion_in_switch_word_in_word_context() {
        let mut lexer = Lexer::with_code(r"-~}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(
                switch.word.units,
                [WordUnit::Tilde {
                    name: "".to_string(),
                    followed_by_slash: false
                }]
            );
        });
    }

    #[test]
    fn lexer_suffix_modifier_tilde_expansion_in_switch_word_in_text_context() {
        let mut lexer = Lexer::with_code(r"-~}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Switch(switch) => {
            assert_eq!(
                switch.word.units,
                [WordUnit::Unquoted(TextUnit::Literal('~'))]
            );
        });
    }

    #[test]
    fn lexer_suffix_modifier_trim_shortest_prefix_in_word_context() {
        let mut lexer = Lexer::with_code("#'*'}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(trim.pattern.units, [WordUnit::SingleQuote("*".to_string())]);
            assert_eq!(*trim.pattern.location.code.value.borrow(), "#'*'}");
            assert_eq!(trim.pattern.location.range, 1..4);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_shortest_prefix_in_text_context() {
        let mut lexer = Lexer::with_code("#'*'}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Text,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(trim.pattern.units, [WordUnit::SingleQuote("*".to_string())]);
            assert_eq!(*trim.pattern.location.code.value.borrow(), "#'*'}");
            assert_eq!(trim.pattern.location.range, 1..4);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_longest_prefix() {
        let mut lexer = Lexer::with_code(r#"##"?"}"#);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Longest);
            assert_eq!(trim.pattern.units.len(), 1, "{:?}", trim.pattern);
            assert_matches!(&trim.pattern.units[0], WordUnit::DoubleQuote(Text(units)) => {
                assert_eq!(units[..], [TextUnit::Literal('?')]);
            });
            assert_eq!(*trim.pattern.location.code.value.borrow(), r#"##"?"}"#);
            assert_eq!(trim.pattern.location.range, 2..5);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_shortest_suffix() {
        let mut lexer = Lexer::with_code(r"%\%}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Suffix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(
                trim.pattern.units,
                [WordUnit::Unquoted(TextUnit::Backslashed('%'))]
            );
            assert_eq!(*trim.pattern.location.code.value.borrow(), r"%\%}");
            assert_eq!(trim.pattern.location.range, 1..3);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_trim_longest_suffix() {
        let mut lexer = Lexer::with_code("%%%}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Suffix);
            assert_eq!(trim.length, TrimLength::Longest);
            assert_eq!(
                trim.pattern.units,
                [WordUnit::Unquoted(TextUnit::Literal('%'))]
            );
            assert_eq!(*trim.pattern.location.code.value.borrow(), "%%%}");
            assert_eq!(trim.pattern.location.range, 2..3);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('}')));
    }

    #[test]
    fn lexer_suffix_modifier_tilde_expansion_in_trim_word() {
        let mut lexer = Lexer::with_code(r"#~}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.suffix_modifier().now_or_never().unwrap().unwrap();
        assert_matches!(result, Modifier::Trim(trim) => {
            assert_eq!(
                trim.pattern.units,
                [WordUnit::Tilde {
                    name: "".to_string(),
                    followed_by_slash: false
                }]
            );
        });
    }

    #[test]
    fn lexer_suffix_modifier_orphan_colon_eof() {
        let mut lexer = Lexer::with_code(r":");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = lexer.suffix_modifier().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidModifier));
        assert_eq!(*e.location.code.value.borrow(), ":");
        assert_eq!(e.location.range, 0..1);
    }

    #[test]
    fn lexer_suffix_modifier_orphan_colon_followed_by_letter() {
        let mut lexer = Lexer::with_code(r":x}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = lexer.suffix_modifier().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidModifier));
        assert_eq!(*e.location.code.value.borrow(), ":x}");
        assert_eq!(e.location.range, 0..1);
    }

    #[test]
    fn lexer_suffix_modifier_orphan_colon_followed_by_symbol() {
        let mut lexer = Lexer::with_code(r":#}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let e = lexer.suffix_modifier().now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidModifier));
        assert_eq!(*e.location.code.value.borrow(), ":#}");
        assert_eq!(e.location.range, 0..2);
    }
}
