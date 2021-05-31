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
use super::core::WordLexer;
use crate::parser::core::Error;
use crate::parser::core::Result;
use crate::parser::core::SyntaxError;
use crate::syntax::Modifier;
use crate::syntax::Switch;
use crate::syntax::SwitchCondition;
use crate::syntax::SwitchType;
use crate::syntax::Word;
use std::future::Future;
use std::pin::Pin;

impl Lexer {
    async fn suffix_modifier_not_found(&mut self, colon: bool) -> Result<Modifier> {
        if colon {
            let cause = SyntaxError::InvalidModifier.into();
            let location = self.location().await?.clone();
            Err(Error { cause, location })
        } else {
            Ok(Modifier::None)
        }
    }
}

impl WordLexer<'_> {
    /// Parses a suffix modifier, i.e., a modifier other than the length prefix.
    pub async fn suffix_modifier(&mut self) -> Result<Modifier> {
        let colon = self.skip_if(|c| c == ':').await?;

        let r#type = if let Some(c) = self.peek_char().await? {
            match c.value {
                '+' => SwitchType::Alter,
                '-' => SwitchType::Default,
                '=' => SwitchType::Assign,
                '?' => SwitchType::Error,
                _ => return self.suffix_modifier_not_found(colon).await,
            }
        } else {
            return self.suffix_modifier_not_found(colon).await;
        };
        self.consume_char();

        let condition = if colon {
            SwitchCondition::UnsetOrEmpty
        } else {
            SwitchCondition::Unset
        };

        // Boxing needed for recursion
        let word = Box::pin(self.word(|c| c == '}')) as Pin<Box<dyn Future<Output = Result<Word>>>>;
        let word = word.await?;

        let switch = Switch {
            r#type,
            condition,
            word,
        };
        Ok(Modifier::Switch(switch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::core::ErrorCause;
    use crate::parser::lex::WordContext;
    use crate::source::Source;
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '}');
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
    fn lexer_suffix_modifier_orphan_colon() {
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
}
