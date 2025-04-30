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

//! Part of the lexer that parses braced parameter expansion

use super::core::WordLexer;
use super::raw_param::is_portable_name;
use super::raw_param::is_portable_name_char;
use crate::parser::core::Result;
use crate::parser::error::Error;
use crate::parser::error::SyntaxError;
use crate::syntax::BracedParam;
use crate::syntax::Modifier;
use crate::syntax::Param;
use crate::syntax::ParamType;
use crate::syntax::SpecialParam;
use std::num::IntErrorKind;

/// Tests if a character can be part of a variable name.
///
/// The current implementation is the same as [`is_portable_name_char`].
/// Other (POSIXly non-portable) characters may be supported in the future.
pub fn is_name_char(c: char) -> bool {
    // TODO support other Unicode name characters
    is_portable_name_char(c)
}

/// Tests if a string is a valid name.
///
/// The current implementation is the same as [`is_portable_name`].
/// Other (POSIXly non-portable) characters may be allowed in the future.
pub fn is_name(s: &str) -> bool {
    // TODO support other Unicode name characters
    is_portable_name(s)
}

/// Determines the type of the parameter.
///
/// This function assumes the argument contains [name characters](is_name_char)
/// only.
///
/// - If the argument does not start with a digit, it is a named parameter.
/// - Otherwise, it is a positional parameter.
///   However, if it contains non-digit characters, it is an error.
///
/// This function does not care for special parameters other than `0`.
/// The special parameter `0` is recognized only if the argument is exactly
/// a single-digit `0`, as required by POSIX.
#[must_use]
fn type_of_id(id: &str) -> Option<ParamType> {
    if id == "0" {
        return Some(ParamType::Special(SpecialParam::Zero));
    }
    if id.starts_with(|c: char| c.is_ascii_digit()) {
        return match id.parse() {
            Ok(index) => Some(ParamType::Positional(index)),
            Err(e) => match e.kind() {
                IntErrorKind::PosOverflow => Some(ParamType::Positional(usize::MAX)),
                _ => None,
            },
        };
    }
    Some(ParamType::Variable)
}

impl WordLexer<'_, '_> {
    /// Tests if there is a length prefix (`#`).
    ///
    /// This function may consume many characters, possibly beyond the length
    /// prefix, regardless of the result. The caller should remember the index
    /// before calling this function and rewind afterwards.
    async fn has_length_prefix(&mut self) -> Result<bool> {
        if !self.skip_if(|c| c == '#').await? {
            return Ok(false);
        }

        // A parameter expansion cannot have both a prefix and suffix modifier.
        // For example, `${#-?}` is not considered to have a prefix. We need to
        // look ahead to see if it is okay to treat the `#` as a prefix.
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
    pub async fn braced_param(&mut self, start_index: usize) -> Result<Option<BracedParam>> {
        if !self.skip_if(|c| c == '{').await? {
            return Ok(None);
        }

        let opening_location = self.location_range(start_index..self.index());

        let has_length_prefix = self.length_prefix().await?;

        let param_start_index = self.index();

        let c = self.peek_char().await?.unwrap();
        let param = if is_name_char(c) {
            self.consume_char();

            // Parse the remaining characters of the parameter name
            let mut id = c.to_string();
            while let Some(c) = self.consume_char_if(is_name_char).await? {
                id.push(c.value);
            }

            let Some(r#type) = type_of_id(&id) else {
                let cause = SyntaxError::InvalidParam.into();
                let location = self.location_range(param_start_index..self.index());
                return Err(Error { cause, location });
            };
            Param { id, r#type }
        } else if let Some(special) = SpecialParam::from_char(c) {
            self.consume_char();
            Param {
                id: c.to_string(),
                r#type: special.into(),
            }
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

        Ok(Some(BracedParam {
            param,
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
        let mut lexer = Lexer::with_code("$foo");
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
        let mut lexer = Lexer::with_code("${@};");
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::At));
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
        let mut lexer = Lexer::with_code("X${foo_123}<");
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
        assert_eq!(param.param, Param::variable("foo_123"));
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "X${foo_123}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 1..11);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_positional() {
        let mut lexer = Lexer::with_code("${123}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(123));
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${123}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..6);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    /// Tests that the parameter expansion `${00}` is parsed as a positional
    /// parameter with the index 0. Compare [`lexer_braced_param_special_zero`].
    #[test]
    fn lexer_braced_param_positional_zero() {
        let mut lexer = Lexer::with_code("${00}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param.id, "00");
        assert_eq!(param.param.r#type, ParamType::Positional(0));
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${00}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..5);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_positional_overflow() {
        // This overflow is reported at the execution time of the script, not at
        // the parsing time.
        let mut lexer = Lexer::with_code("${9999999999999999999999999999999999999999}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param.r#type, ParamType::Positional(usize::MAX));
    }

    #[test]
    fn lexer_braced_param_invalid_param() {
        let mut lexer = Lexer::with_code("${0_0}");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let e = lexer.braced_param(0).now_or_never().unwrap().unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::InvalidParam));
        assert_eq!(*e.location.code.value.borrow(), "${0_0}");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(*e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..5);
    }

    /// Tests that the parameter expansion `${0}` is parsed as a special
    /// parameter `0`. Compare [`lexer_braced_param_positional_zero`].
    #[test]
    fn lexer_braced_param_special_zero() {
        let mut lexer = Lexer::with_code("${0}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param.id, "0");
        assert_eq!(param.param.r#type, ParamType::Special(SpecialParam::Zero));
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${0}<");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..4);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_special_hash() {
        let mut lexer = Lexer::with_code("${#}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${};");
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
        let mut lexer = Lexer::with_code("${;");
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
        let mut lexer = Lexer::with_code("${_;");
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
        let mut lexer = Lexer::with_code("${#foo_123}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::variable("foo_123"));
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
        let mut lexer = Lexer::with_code("${##}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#?}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Question));
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
        let mut lexer = Lexer::with_code("${#-}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Hyphen));
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
        let mut lexer = Lexer::with_code("${x+})");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::variable("x"));
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
        let mut lexer = Lexer::with_code("${foo:?'!'})");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::variable("foo"));
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
        let mut lexer = Lexer::with_code("${#+?}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#--}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#=?}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#??}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#:-}<");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${###};");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#%};");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
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
        let mut lexer = Lexer::with_code("${#x+};");
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
        let mut lexer = Lexer::with_code("${\\\n#\\\n\\\na_\\\n1\\\n\\\n}z");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::variable("a_1"));
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
        let mut lexer = Lexer::with_code("${#\\\n\\\n}z");
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.braced_param(0).now_or_never().unwrap();
        let param = result.unwrap().unwrap();
        assert_eq!(param.param, Param::from(SpecialParam::Number));
        assert_eq!(param.modifier, Modifier::None);
        // TODO assert about other param members
        assert_eq!(*param.location.code.value.borrow(), "${#\\\n\\\n}z");
        assert_eq!(param.location.code.start_line_number.get(), 1);
        assert_eq!(*param.location.code.source, Source::Unknown);
        assert_eq!(param.location.range, 0..8);

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('z')));
    }
}
