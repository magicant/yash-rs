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
use crate::source::Location;
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
    /// the return value is `Ok(Err(location))`.
    ///
    /// The `location` parameter should be the location of the initial `$`. It
    /// is used to construct the result, but this function does not check if it
    /// actually is a location of `$`.
    pub async fn braced_param(
        &mut self,
        location: Location,
    ) -> Result<std::result::Result<Param, Location>> {
        if !self.skip_if(|c| c == '{').await? {
            return Ok(Err(location));
        }

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
        } else if c == '}' {
            let cause = SyntaxError::EmptyParam.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        } else {
            let opening_location = location;
            let cause = SyntaxError::UnclosedParam { opening_location }.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        };

        let suffix_location = self.location().await?.clone();
        let suffix = self.suffix_modifier().await?;

        if !self.skip_if(|c| c == '}').await? {
            let opening_location = location;
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

        Ok(Ok(Param {
            name,
            modifier,
            location,
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
    use futures_executor::block_on;

    fn assert_opening_location(location: &Location) {
        assert_eq!(*location.code.value.borrow(), "$");
        assert_eq!(location.code.start_line_number.get(), 1);
        assert_eq!(location.code.source, Source::Unknown);
        assert_eq!(location.range, 0..1);
    }

    #[test]
    fn lexer_braced_param_minimum() {
        let mut lexer = Lexer::from_memory("{@};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "@");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_braced_param_alphanumeric_name() {
        let mut lexer = Lexer::from_memory("{foo_123}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "foo_123");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_numeric_name() {
        let mut lexer = Lexer::from_memory("{123}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "123");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash() {
        let mut lexer = Lexer::from_memory("{#}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_missing_name() {
        let mut lexer = Lexer::from_memory("{};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyParam));
        assert_eq!(*e.location.code.value.borrow(), "{};");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 0..2);
    }

    #[test]
    fn lexer_braced_param_unclosed_without_name() {
        let mut lexer = Lexer::from_memory("{;", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedParam { opening_location }) => {
            assert_opening_location(&opening_location);
        });
        assert_eq!(*e.location.code.value.borrow(), "{;");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 1..2);
    }

    #[test]
    fn lexer_braced_param_unclosed_with_name() {
        let mut lexer = Lexer::from_memory("{_;", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        assert_matches!(e.cause,
            ErrorCause::Syntax(SyntaxError::UnclosedParam { opening_location }) => {
            assert_opening_location(&opening_location);
        });
        assert_eq!(*e.location.code.value.borrow(), "{_;");
        assert_eq!(e.location.code.start_line_number.get(), 1);
        assert_eq!(e.location.code.source, Source::Unknown);
        assert_eq!(e.location.range, 2..3);
    }

    #[test]
    fn lexer_braced_param_length_alphanumeric_name() {
        let mut lexer = Lexer::from_memory("{#foo_123}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "foo_123");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_length_hash() {
        let mut lexer = Lexer::from_memory("{##}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_length_question() {
        let mut lexer = Lexer::from_memory("{#?}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "?");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_length_hyphen() {
        let mut lexer = Lexer::from_memory("{#-}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "-");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_switch_minimum() {
        let mut lexer = Lexer::from_memory("{x+})", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "x");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(')')));
    }

    #[test]
    fn lexer_braced_param_switch_full() {
        let mut lexer = Lexer::from_memory("{foo:?'!'})", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "foo");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Error);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.to_string(), "'!'");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(')')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_alter() {
        let mut lexer = Lexer::from_memory("{#+?}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Alter);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "?");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_default() {
        let mut lexer = Lexer::from_memory("{#--}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Default);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "-");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_assign() {
        let mut lexer = Lexer::from_memory("{#=?}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Assign);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "?");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_error() {
        let mut lexer = Lexer::from_memory("{#??}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Error);
            assert_eq!(switch.condition, SwitchCondition::Unset);
            assert_eq!(switch.word.to_string(), "?");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_suffix_with_colon() {
        let mut lexer = Lexer::from_memory("{#:-}<", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Switch(switch) => {
            assert_eq!(switch.r#type, SwitchType::Default);
            assert_eq!(switch.condition, SwitchCondition::UnsetOrEmpty);
            assert_eq!(switch.word.to_string(), "");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_braced_param_hash_with_longest_prefix_trim() {
        let mut lexer = Lexer::from_memory("{###};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Prefix);
            assert_eq!(trim.length, TrimLength::Longest);
            assert_eq!(trim.pattern.to_string(), "");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_braced_param_hash_with_suffix_trim() {
        let mut lexer = Lexer::from_memory("{#%};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_matches!(result.modifier, Modifier::Trim(trim) => {
            assert_eq!(trim.side, TrimSide::Suffix);
            assert_eq!(trim.length, TrimLength::Shortest);
            assert_eq!(trim.pattern.to_string(), "");
        });
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_braced_param_multiple_modifier() {
        let mut lexer = Lexer::from_memory("{#x+};", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::MultipleModifier));
        assert_eq!(*e.location.code.value.borrow(), "{#x+};");
        assert_eq!(e.location.range, 3..4);
    }

    #[test]
    fn lexer_braced_param_line_continuations() {
        let mut lexer = Lexer::from_memory("{\\\n#\\\n\\\na_\\\n1\\\n\\\n}z", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "a_1");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('z')));
    }

    #[test]
    fn lexer_braced_param_line_continuations_hash() {
        let mut lexer = Lexer::from_memory("{#\\\n\\\n}z", Source::Unknown);
        let mut lexer = WordLexer {
            lexer: &mut lexer,
            context: WordContext::Word,
        };
        let location = Location::dummy("$");

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('z')));
    }
}
