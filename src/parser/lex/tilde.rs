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

//! Tilde expansion parser.
//!
//! TODO Elaborate

use crate::syntax::TextUnit::Literal;
use crate::syntax::Word;
use crate::syntax::WordUnit::{Tilde, Unquoted};

impl Word {
    /// TODO
    pub fn parse_tilde(self) -> Self {
        if self.units.first() != Some(&Unquoted(Literal('~'))) {
            return self;
        }

        let mut i = self.units.into_iter().peekable();
        let tilde = i.next().unwrap();
        debug_assert_eq!(tilde, Unquoted(Literal('~')));

        // Parse the body of the tilde expansion into `name`, consuming characters from `i`.
        let mut name = String::new();
        while let Some(Unquoted(Literal(c))) =
            i.next_if(|unit| matches!(unit, Unquoted(Literal(c)) if !matches!(*c, '/' | ':')))
        {
            name.push(c)
        }

        // Check the delimiter and create the result.
        let mut units = match i.peek() {
            None | Some(Unquoted(Literal(_))) => vec![Tilde(name)],
            Some(_) => {
                // The next word unit is not applicable for tilde expansion.
                // Revert to the original literals.
                let mut units = vec![Unquoted(Literal('~'))];
                units.extend(name.chars().map(|c| Unquoted(Literal(c))));
                units
            }
        };
        units.extend(i);
        Word { units, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Text;
    use crate::syntax::TextUnit::Backslashed;
    use crate::syntax::WordUnit::{DoubleQuote, SingleQuote};
    use std::str::FromStr;

    #[test]
    fn word_parse_tilde_not_starting_with_tilde() {
        let input = Word::from_str("").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result, input);

        let input = Word::from_str("a").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result, input);

        let input = Word::from_str("''").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result, input);
    }

    #[test]
    fn word_parse_tilde_only_tilde() {
        let input = Word::from_str("~").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result.location, input.location);
        assert_eq!(result.units, [Tilde("".to_string())]);
    }

    #[test]
    fn word_parse_tilde_with_name() {
        let input = Word::from_str("~foo").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result.location, input.location);
        assert_eq!(result.units, [Tilde("foo".to_string())]);
    }

    #[test]
    fn word_parse_tilde_ending_with_slash() {
        let input = Word::from_str("~bar/''").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde("bar".to_string()),
                Unquoted(Literal('/')),
                SingleQuote("".to_string()),
            ]
        );
    }

    #[test]
    fn word_parse_tilde_ending_with_colon() {
        let input = Word::from_str("~bar:\"\"").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde("bar".to_string()),
                Unquoted(Literal(':')),
                DoubleQuote(Text(vec![])),
            ]
        );
    }

    #[test]
    fn word_parse_tilde_interrupted_by_non_literal() {
        let input = Word::from_str(r"~foo\/").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Unquoted(Literal('~')),
                Unquoted(Literal('f')),
                Unquoted(Literal('o')),
                Unquoted(Literal('o')),
                Unquoted(Backslashed('/')),
            ]
        );

        let input = Word::from_str("~bar''").unwrap();
        let result = input.clone().parse_tilde();
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Unquoted(Literal('~')),
                Unquoted(Literal('b')),
                Unquoted(Literal('a')),
                Unquoted(Literal('r')),
                SingleQuote("".to_string()),
            ]
        );
    }
}
