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

use super::core::Lexer;
use super::raw_param::is_portable_name_char;
use super::raw_param::is_special_parameter_char;
use crate::parser::core::Error;
use crate::parser::core::Result;
use crate::parser::core::SyntaxError;
use crate::source::Location;
use crate::source::SourceChar;
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

impl Lexer {
    /// Consumes a length prefix (`#`) if any.
    async fn length_prefix(&mut self) -> Result<bool> {
        let index = self.index();
        if !self.skip_if(|c| c == '#').await? {
            return Ok(false);
        }

        if let Some(&SourceChar { value: '}', .. }) = self.peek_char().await? {
            self.rewind(index);
            Ok(false)
        } else {
            Ok(true)
        }
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
    ///
    /// This function does not consume line continuations after the initial `$`.
    /// They should have been consumed beforehand.
    pub async fn braced_param(
        &mut self,
        location: Location,
    ) -> Result<std::result::Result<Param, Location>> {
        if !self.skip_if(|c| c == '{').await? {
            return Ok(Err(location));
        }

        let has_length_prefix = self.length_prefix().await?;

        let sc = self.peek_char().await?.unwrap();
        let c = sc.value;
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
            let location = sc.location.clone();
            return Err(Error { cause, location });
        } else {
            let opening_location = location;
            let cause = SyntaxError::UnclosedParam { opening_location }.into();
            let location = sc.location.clone();
            return Err(Error { cause, location });
        };

        if !self.skip_if(|c| c == '}').await? {
            let opening_location = location;
            let cause = SyntaxError::UnclosedParam { opening_location }.into();
            let location = self.location().await?.clone();
            return Err(Error { cause, location });
        }

        let modifier = if has_length_prefix {
            Modifier::Length
        } else {
            Modifier::None
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
    use crate::parser::core::ErrorCause;
    use crate::source::Source;
    use futures::executor::block_on;

    fn assert_opening_location(location: &Location) {
        assert_eq!(location.line.value, "$");
        assert_eq!(location.line.number.get(), 1);
        assert_eq!(location.line.source, Source::Unknown);
        assert_eq!(location.column.get(), 1);
    }

    #[test]
    fn lexer_braced_param_minimum() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{@};");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "@");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, ';');
    }

    #[test]
    fn lexer_braced_param_alphanumeric_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{foo_123}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "foo_123");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_braced_param_numeric_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{123}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "123");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_braced_param_hash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{#}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_braced_param_missing_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{};");
        let location = Location::dummy("$".to_string());

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        assert_eq!(e.cause, ErrorCause::Syntax(SyntaxError::EmptyParam));
        assert_eq!(e.location.line.value, "{};");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn lexer_braced_param_unclosed_without_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{;");
        let location = Location::dummy("$".to_string());

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedParam { opening_location }) = e.cause {
            assert_opening_location(&opening_location);
        } else {
            panic!("Unexpected cause: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "{;");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 2);
    }

    #[test]
    fn lexer_braced_param_unclosed_with_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{_;");
        let location = Location::dummy("$".to_string());

        let e = block_on(lexer.braced_param(location)).unwrap_err();
        if let ErrorCause::Syntax(SyntaxError::UnclosedParam { opening_location }) = e.cause {
            assert_opening_location(&opening_location);
        } else {
            panic!("Unexpected cause: {:?}", e.cause);
        }
        assert_eq!(e.location.line.value, "{_;");
        assert_eq!(e.location.line.number.get(), 1);
        assert_eq!(e.location.line.source, Source::Unknown);
        assert_eq!(e.location.column.get(), 3);
    }

    #[test]
    fn lexer_braced_param_length_alphanumeric_name() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{#foo_123}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "foo_123");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_braced_param_length_hash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{##}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_braced_param_length_question() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{#?}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "?");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    #[test]
    fn lexer_braced_param_length_hyphen() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{#-}<");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "-");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, '<');
    }

    // TODO ${###} ${#%}

    #[test]
    fn lexer_braced_param_line_continuations() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{\\\n#\\\n\\\na_\\\n1\\\n\\\n}z");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "a_1");
        assert_eq!(result.modifier, Modifier::Length);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, 'z');
    }

    #[test]
    fn lexer_braced_param_line_continuations_hash() {
        let mut lexer = Lexer::with_source(Source::Unknown, "{#\\\n\\\n}z");
        let location = Location::dummy("$".to_string());

        let result = block_on(lexer.braced_param(location)).unwrap().unwrap();
        assert_eq!(result.name, "#");
        assert_eq!(result.modifier, Modifier::None);
        // TODO assert about other result members
        assert_opening_location(&result.location);

        assert_eq!(block_on(lexer.peek_char()).unwrap().unwrap().value, 'z');
    }
}
