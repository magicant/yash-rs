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

//! Part of the lexer that parses raw parameter expansion

use super::core::Lexer;
use crate::parser::core::Result;
use crate::syntax::Param;
use crate::syntax::ParamType;
use crate::syntax::SpecialParam;
use crate::syntax::TextUnit;

/// Tests if a character can be part of a POSIXly-portable name.
///
/// Returns true if the character is an ASCII alphanumeric or underscore.
///
/// Note that a valid name cannot start with a digit, but this function
/// returns true for digits as well.
///
/// Use [`is_portable_name`] to check if a string is a valid name.
pub const fn is_portable_name_char(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='Z' | '_' | 'a'..='z')
}

/// Tests if a string is a valid POSIXly-portable name.
///
/// Returns true if the string is non-empty, the first character is not a digit,
/// and all characters are ASCII alphanumeric or underscore.
///
/// Use [`is_portable_name_char`] to check each character.
pub fn is_portable_name(s: &str) -> bool {
    s.starts_with(|c: char| !c.is_ascii_digit()) && s.chars().all(is_portable_name_char)
}

/// Tests if a character names a special parameter.
///
/// A special parameter is one of: `@*#?-$!0`.
pub const fn is_special_parameter_char(c: char) -> bool {
    SpecialParam::from_char(c).is_some()
}

/// Tests if a character is a valid single-character raw parameter.
///
/// If this function returns true, the character is a valid parameter for a raw
/// parameter expansion, but the next character is never treated as part of the
/// parameter.
///
/// This function returns true for ASCII digits and special parameters.
pub const fn is_single_char_name(c: char) -> bool {
    c.is_ascii_digit() || is_special_parameter_char(c)
}

impl Lexer<'_> {
    /// Parses a parameter expansion that is not enclosed in braces.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// This functions checks if the next character is a valid POSIXly-portable
    /// parameter name. If so, the name is consumed and returned. Otherwise, no
    /// characters are consumed and the return value is `Ok(None)`.
    ///
    /// The `start_index` parameter should be the index for the initial `$`. It is
    /// used to construct the result, but this function does not check if it
    /// actually points to the `$`.
    pub async fn raw_param(&mut self, start_index: usize) -> Result<Option<TextUnit>> {
        let param = if let Some(c) = self.consume_char_if(is_special_parameter_char).await? {
            Param {
                id: c.value.to_string(),
                r#type: SpecialParam::from_char(c.value).unwrap().into(),
            }
        } else if let Some(c) = self.consume_char_if(|c| c.is_ascii_digit()).await? {
            Param {
                id: c.value.to_string(),
                r#type: ParamType::Positional(c.value.to_digit(10).unwrap() as usize),
            }
        } else if let Some(c) = self.consume_char_if(is_portable_name_char).await? {
            let mut name = c.value.to_string();
            while let Some(c) = self.consume_char_if(is_portable_name_char).await? {
                name.push(c.value);
            }
            Param::variable(name)
        } else {
            return Ok(None);
        };

        let location = self.location_range(start_index..self.index());

        Ok(Some(TextUnit::RawParam { param, location }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use crate::syntax::Param;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;

    #[test]
    fn test_is_portable_name() {
        assert!(!is_portable_name(""));
        assert!(is_portable_name("valid_name"));
        assert!(!is_portable_name("1invalid_name"));
        assert!(is_portable_name("valid_name_123"));
        assert!(is_portable_name("_VALID_NAME"));
    }

    #[test]
    fn lexer_raw_param_special_parameter() {
        let mut lexer = Lexer::with_code("$@;");
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.raw_param(0).now_or_never().unwrap().unwrap().unwrap();
        assert_matches!(result, TextUnit::RawParam { param, location } => {
            assert_eq!(param, Param::from(SpecialParam::At));
            assert_eq!(*location.code.value.borrow(), "$@;");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..2);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }

    #[test]
    fn lexer_raw_param_digit() {
        let mut lexer = Lexer::with_code("$12");
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.raw_param(0).now_or_never().unwrap().unwrap().unwrap();
        assert_matches!(result, TextUnit::RawParam { param, location } => {
            assert_eq!(param, Param::from(1));
            assert_eq!(*location.code.value.borrow(), "$12");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..2);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('2')));
    }

    #[test]
    fn lexer_raw_param_posix_name() {
        let mut lexer = Lexer::with_code("$az_AZ_019<");
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.raw_param(0).now_or_never().unwrap().unwrap().unwrap();
        assert_matches!(result, TextUnit::RawParam { param, location } => {
            assert_eq!(param, Param::variable("az_AZ_019"));
            assert_eq!(*location.code.value.borrow(), "$az_AZ_019<");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..10);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('<')));
    }

    #[test]
    fn lexer_raw_param_posix_name_line_continuations() {
        let mut lexer = Lexer::with_code("$a\\\n\\\nb\\\n\\\nc\\\n>");
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();

        let result = lexer.raw_param(0).now_or_never().unwrap().unwrap().unwrap();
        assert_matches!(result, TextUnit::RawParam { param, location } => {
            assert_eq!(param, Param::variable("abc"));
            assert_eq!(*location.code.value.borrow(), "$a\\\n\\\nb\\\n\\\nc\\\n>");
            assert_eq!(location.code.start_line_number.get(), 1);
            assert_eq!(*location.code.source, Source::Unknown);
            assert_eq!(location.range, 0..14);
        });

        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some('>')));
    }

    #[test]
    fn lexer_raw_param_not_parameter() {
        let mut lexer = Lexer::with_code("$;");
        lexer.peek_char().now_or_never().unwrap().unwrap();
        lexer.consume_char();
        assert_eq!(lexer.raw_param(0).now_or_never().unwrap(), Ok(None));
        assert_eq!(lexer.peek_char().now_or_never().unwrap(), Ok(Some(';')));
    }
}
