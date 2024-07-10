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

//! Part of the lexer that parses braced parameter expansion.

use super::core::WordLexer;
use super::raw_param::is_portable_name_char;
use super::raw_param::is_special_parameter_char;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::syntax::Modifier;
use crate::syntax::Param;

/// Tests if a character can be part of a variable name.
///
/// The current implementation is the same as [`is_portable_name_char`].
/// Other (POSIXly non-portable) characters may be supported in the future.
pub fn is_name_char(c: char) -> bool {
    // TODO support other Unicode name characters
    is_portable_name_char(c)
}

impl WordLexer<'_, '_> {
    /// Tests if there is a length prefix (`#`).
    ///
    /// This function may consume many characters, possibly beyond the length
    /// prefix, regardless of the result. The caller should rewind to the index
    /// this function returns.
    async fn has_length_prefix(&mut self) -> Result<bool> {
        if !self.skip_if(|c| c == '#').await? {
            return Ok(false);
        }

        // Remember that a parameter expansion cannot have both a prefix and
        // suffix modifier. For example, `${#-?}` is not considered to have a
        // prefix. We need to look ahead to see if it is okay to treat the `#`
        // as a prefix.
        if let Some(c) = self.peek_char().await? {
            // Check characters that cannot be a special parameter.
            if matches!(c, '}' | '+' | '=' | ':' | '%') {
                return Ok(false);
            }

            // Check characters that can be either a special parameter or the
            // beginning of a modifier
            if matches!(c, '-' | '?' | '#') {
                self.consume_char();
                if let Some(c) = self.peek_char().await? {
                    return Ok(c == '}');
                }
            }
        }

        Ok(true)
    }

    /// Consumes a length prefix (`#`) if any.
    async fn length_prefix(&mut self) -> Result<bool> {
        let initial_index = self.index();
        let has_length_prefix = self.has_length_prefix().await?;
        self.rewind(initial_index);
        if has_length_prefix {
            self.peek_char().await?;
            self.consume_char();
        }
        Ok(has_length_prefix)
    }

    /// Parses a parameter expansion that is enclosed in braces.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// This functions checks if the next character is an opening brace. If so,
    /// the following characters are parsed as a parameter expansion up to and
    /// including the closing brace. Otherwise, no characters are consumed and
    /// the return value is `Ok(None)`.
    ///
    /// The `start_index` parameter should be the index for the initial `$`. It is
    /// used to construct the result, but this function does not check if it
    /// actually points to the `$`.
    pub async fn braced_param(&mut self, start_index: usize) -> Result<Option<Param>> {
        if !self.skip_if(|c| c == '{').await? {
            return Ok(None);
        }

        let opening_location = self.location_range(start_index..self.index());

        let has_length_prefix = self.length_prefix().await?;

        let c = self.peek_char().await?.unwrap();
        let name = if is_special_parameter_char(c) {
            self.consume_char();
            c.to_string()
        } else if is_name_char(c) {
            self.consume_char();
            let mut name = c.to_string();
            while let Some(c) = self.consume_char_if(is_name_char).await? {
                name.push(c.value);
            }
            name
        } else {
            let cause = SyntaxError::EmptyParam.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        };

        let suffix_location = self.location().await?.clone();
        let suffix = self.suffix_modifier().await?;

        if !self.skip_if(|c| c == '}').await? {
            let cause = SyntaxError::UnclosedParam { opening_location }.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        }

        let modifier = match (has_length_prefix, suffix) {
            (true, Modifier::None) => Modifier::Length,
            (true, _) => {
                let cause = SyntaxError::MultipleModifier.into();
                let location = suffix_location;
                return Err(Error { cause, location });
            }
            (false, suffix) => suffix,
        };

        Ok(Some(Param {
            name,
            modifier,
            location: self.location_range(start_index..self.index()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorCause;
    use crate::parser::lex::Lexer;
    use crate::parser::lex::WordContext;
    use crate::source::Source;
    use crate::syntax::SwitchCondition;
    use crate::syntax::SwitchType;
    use crate::syntax::TrimLength;
    use crate::syntax::TrimSide;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn lexer_braced_param_none() {
        let mut lexer = Lexer::from_memory("$foo", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        assert_eq!(lexer.braced_param(0).now_or_never().unwrap(), Ok(None));
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('f')));
    }

    #[test]
    fn lexer_braced_param_minimum() {
        let mut lexer = Lexer::from_memory("${@};", Source::Unknown);
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "@");
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${@};");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..4);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_braced_param_alphanumeric_name() {
        let mut lexer = Lexer::from_memory("X${foo_123}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(1).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "foo_123");
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "X${foo_123}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 1..11);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_numeric_name() {
        let mut lexer = Lexer::from_memory("${123}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "123");
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${123}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash() {
        let mut lexer = Lexer::from_memory("${#}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..4);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_missing_name() {
        let mut lexer = Lexer::from_memory("${};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let e = lexer.braced_param(0).now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyParam));
        assert_eq!(*e.location.code.value.borrow(), "${};");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }

    #[test]
    fn lexer_braced_param_unclosed_without_name() {
        let mut lexer = Lexer::from_memory("${;", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let e = lexer.braced_param(0).now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyParam));
        assert_eq!(*e.location.code.value.borrow(), "${;");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }

    #[test]
    fn lexer_braced_param_unclosed_with_name() {
        let mut lexer = Lexer::from_memory("${_;", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let e = lexer.braced_param(0).now_or_never().unwrap().unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedParam { opening_location }) => {
            assert_eq!(*opening_location.code.value.borrow(), "${_;");
            assert_eq!(opening_location.code.start_line_number.get(), 1);
            assert_eq!(*opening_location.code.source, Source::Unknown);
            assert_eq!(opening_location.range, 0..2);
        });
        assert_eq!(*e.location.code.value.borrow(), "${_;");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 3..4);
    }

    #[test]
    fn lexer_braced_param_length_alphanumeric_name() {
        let mut lexer = Lexer::from_memory("${#foo_123}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "foo_123");
        assert_eq!(param.modifier, Modifier::Length);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#foo_123}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..11);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_length_hash() {
        let mut lexer = Lexer::from_memory("${##}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_eq!(param.modifier, Modifier::Length);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${##}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..5);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_length_question() {
        let mut lexer = Lexer::from_memory("${#?}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "?");
        assert_eq!(param.modifier, Modifier::Length);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#?}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..5);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_length_hyphen() {
        let mut lexer = Lexer::from_memory("${#-}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "-");
        assert_eq!(param.modifier, Modifier::Length);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#-}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..5);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_switch_minimum() {
        let mut lexer = Lexer::from_memory("${x+})", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "x");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${x+})");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..5);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(')')));
    }

    #[test]
    fn lexer_braced_param_switch_full() {
        let mut lexer = Lexer::from_memory("${foo:?'!'})", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "foo");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Error);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.to_string(), "'!'");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${foo:?'!'})");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..11);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(')')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_alter() {
        let mut lexer = Lexer::from_memory("${#+?}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "?");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#+?}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_default() {
        let mut lexer = Lexer::from_memory("${#--}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Default);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "-");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#--}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_assign() {
        let mut lexer = Lexer::from_memory("${#=?}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Assign);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "?");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#=?}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_error() {
        let mut lexer = Lexer::from_memory("${#??}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Error);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "?");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#??}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_with_colon() {
        let mut lexer = Lexer::from_memory("${#:-}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Default);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.to_string(), "");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#:-}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_with_longest_prefix_trim() {
        let mut lexer = Lexer::from_memory("${###};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Longest);
            assert_eq!(trim.pattern.to_string(), "");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${###};");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_braced_param_hash_with_suffix_trim() {
        let mut lexer = Lexer::from_memory("${#%};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_matches!(param.modifier, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Suffix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(trim.pattern.to_string(), "");
        });
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#%};");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..5);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_braced_param_multiple_modifier() {
        let mut lexer = Lexer::from_memory("${#x+};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let e = lexer.braced_param(0).now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MultipleModifier));
        assert_eq!(*e.location.code.value.borrow(), "${#x+};");
        assert_eq!(e.location.range, 4..5);
    }

    #[test]
    fn lexer_braced_param_line_continuations() {
        let mut lexer = Lexer::from_memory("${\\\n#\\\n\\\na_\\\n1\\\n\\\n}z", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "a_1");
        assert_eq!(param.modifier, Modifier::Length);
        // TODO assert about other param members
        assert_eq!(
            *param.location.code.value.borrow(),
            "${\\\n#\\\n\\\na_\\\n1\\\n\\\n}z"
        );
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..19);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('z')));
    }

    #[test]
    fn lexer_braced_param_line_continuations_hash() {
        let mut lexer = Lexer::from_memory("${#\\\n\\\n}z", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.name, "#");
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#\\\n\\\n}z");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..8);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('z')));
    }
}
