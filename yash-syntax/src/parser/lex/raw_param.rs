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

//! Part of the lexer that parses raw parameter expansion.

use super::core::Lexer;
use crate::parser::core::Result;
use crate::source::Location;
use crate::syntax::TextUnit;

/// Tests if a character can be part of a POSIXly-portable name.
///
/// Returns true if the character is an ASCII alphanumeric or underscore.
///
/// Note that a valid name cannot start with a digit.
pub fn is_portable_name_char(c: char) -> bool {
    matches!(c, '0'..='9' | 'A'..='Z' | '_' | 'a'..='z')
}

/// Tests if a character names a special parameter.
///
/// A special parameter is one of: `@*#?-$!0`.
pub fn is_special_parameter_char(c: char) -> bool {
    matches!(c, '@' | '*' | '#' | '?' | '-' | '$' | '!' | '0')
}

/// Tests if a character is a valid single-character raw parameter name.
///
/// If this function returns true, the character is a valid parameter name for a
/// raw parameter expansion, but the next character is never treated as part of
/// the name.
///
/// This function returns true for ASCII digits and special parameter names.
pub fn is_single_char_name(c: char) -> bool {
    c.is_ascii_digit() || is_special_parameter_char(c)
}

impl Lexer {
    /// Parses a parameter expansion that is not enclosed in braces.
    ///
    /// The initial `$` must have been consumed before calling this function.
    /// This functions checks if the next character is a valid POSIXly-portable
    /// parameter name. If so, the name is consumed and returned. Otherwise, no
    /// characters are consumed and the return value is `Ok(Err(location))`.
    ///
    /// The `location` parameter should be the location of the initial `$`. It
    /// is used to construct the result, but this function does not check if it
    /// actually is a location of `$`.
    pub async fn raw_param(
        &mut self,
        location: Location,
    ) -> Result<std::result::Result<TextUnit, Location>> {
        if let Some(c) = self.consume_char_if(is_single_char_name).await? {
            let name = c.value.to_string();
            Ok(Ok(TextUnit::RawParam { name, location }))
        } else if let Some(c) = self.consume_char_if(is_portable_name_char).await? {
            let mut name = c.value.to_string();
            while let Some(c) = self.consume_char_if(is_portable_name_char).await? {
                name.push(c.value);
            }
            Ok(Ok(TextUnit::RawParam { name, location }))
        } else {
            Ok(Err(location))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use futures::executor::block_on;

    #[test]
    fn lexer_raw_param_special_parameter() {
        let mut lexer = Lexer::with_source(Source::Unknown, "@;");
        let location = Location::dummy("$");

        let result = block_on(lexer.raw_param(location)).unwrap().unwrap();
        if let TextUnit::RawParam { name, location } = result {
            assert_eq!(name, "@");
            assert_eq!(location.line.value, "$");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
        } else {
            panic!("Not a parameter expansion: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }

    #[test]
    fn lexer_raw_param_digit() {
        let mut lexer = Lexer::with_source(Source::Unknown, "12");
        let location = Location::dummy("$");

        let result = block_on(lexer.raw_param(location)).unwrap().unwrap();
        if let TextUnit::RawParam { name, location } = result {
            assert_eq!(name, "1");
            assert_eq!(location.line.value, "$");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
        } else {
            panic!("Not a parameter expansion: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('2')));
    }

    #[test]
    fn lexer_raw_param_posix_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "az_AZ_019<");
        let location = Location::dummy("$");

        let result = block_on(lexer.raw_param(location)).unwrap().unwrap();
        if let TextUnit::RawParam { name, location } = result {
            assert_eq!(name, "az_AZ_019");
            assert_eq!(location.line.value, "$");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
        } else {
            panic!("Not a parameter expansion: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('<')));
    }

    #[test]
    fn lexer_raw_param_posix_name_line_continuations() {
        let mut lexer = Lexer::with_source(Source::Unknown, "a\\\n\\\nb\\\n\\\nc\\\n>");
        let location = Location::dummy("$");

        let result = block_on(lexer.raw_param(location)).unwrap().unwrap();
        if let TextUnit::RawParam { name, location } = result {
            assert_eq!(name, "abc");
            assert_eq!(location.line.value, "$");
            assert_eq!(location.line.number.get(), 1);
            assert_eq!(location.line.source, Source::Unknown);
            assert_eq!(location.column.get(), 1);
        } else {
            panic!("Not a parameter expansion: {:?}", result);
        }

        assert_eq!(block_on(lexer.peek_char()), Ok(Some('>')));
    }

    #[test]
    fn lexer_raw_param_not_parameter() {
        let mut lexer = Lexer::with_source(Source::Unknown, ";");
        let location = Location::dummy("X");

        let location = block_on(lexer.raw_param(location)).unwrap().unwrap_err();
        assert_eq!(location.line.value, "X");
        assert_eq!(location.line.number.get(), 1);
        assert_eq!(location.line.source, Source::Unknown);
        assert_eq!(location.column.get(), 1);

        assert_eq!(block_on(lexer.peek_char()), Ok(Some(';')));
    }
}
