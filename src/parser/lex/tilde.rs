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
use crate::syntax::WordUnit::{self, Tilde, Unquoted};
use std::iter::Peekable;

/// Parses a tilde expansion except the initial tilde.
///
/// Returns the literal string up to the next non-applicable word unit.
fn parse_name<I: Iterator<Item = WordUnit>>(i: &mut Peekable<I>) -> String {
    let mut name = String::new();

    while let Some(Unquoted(Literal(c))) =
        i.next_if(|unit| matches!(unit, Unquoted(Literal(c)) if !matches!(*c, '/' | ':')))
    {
        name.push(c)
    }

    name
}

impl Word {
    fn parse_tilde(&mut self) {
        let mut i = self.units.drain(..).peekable();
        let mut is_after_colon = true;
        let mut units = vec![];

        loop {
            is_after_colon = match i.next() {
                Some(Unquoted(Literal('~'))) if is_after_colon => {
                    let name = parse_name(&mut i);

                    // Check the delimiter and push the result.
                    match i.peek() {
                        None | Some(Unquoted(Literal(_))) => units.push(Tilde(name)),
                        Some(_) => {
                            // The next word unit is not applicable for tilde expansion.
                            // Revert to the original literals.
                            units.push(Unquoted(Literal('~')));
                            units.extend(name.chars().map(|c| Unquoted(Literal(c))));
                        }
                    }

                    false
                }
                Some(unit @ Unquoted(Literal(':'))) => {
                    units.push(unit);
                    true
                }
                Some(unit) => {
                    units.push(unit);
                    false
                }
                None => break,
            }
        }

        drop(i);
        self.units = units;
    }

    /// TODO Describe
    ///
    /// TODO Describe about difference from strictly POSIX-conforming behavior
    #[inline]
    pub fn parse_tilde_front(&mut self) {
        self.parse_tilde()
    }

    /// TODO Describe
    #[inline]
    pub fn parse_tilde_everywhere(&mut self) {
        self.parse_tilde()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Text;
    use crate::syntax::TextUnit::Backslashed;
    use crate::syntax::WordUnit::{DoubleQuote, SingleQuote};
    use std::str::FromStr;

    fn parse_tilde_front(word: &Word) -> Word {
        let mut word = word.clone();
        word.parse_tilde_front();
        word
    }

    fn parse_tilde_everywhere(word: &Word) -> Word {
        let mut word = word.clone();
        word.parse_tilde_everywhere();
        word
    }

    #[test]
    fn word_parse_tilde_front_not_starting_with_tilde() {
        let input = Word::from_str("").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result, input);

        let input = Word::from_str("a").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result, input);

        let input = Word::from_str("''").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn word_parse_tilde_front_only_tilde() {
        let input = Word::from_str("~").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(result.units, [Tilde("".to_string())]);
    }

    #[test]
    fn word_parse_tilde_front_with_name() {
        let input = Word::from_str("~foo").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(result.units, [Tilde("foo".to_string())]);
    }

    #[test]
    fn word_parse_tilde_front_ending_with_slash() {
        let input = Word::from_str("~bar/''").unwrap();
        let result = parse_tilde_front(&input);
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
    fn word_parse_tilde_front_ending_with_colon() {
        let input = Word::from_str("~bar:\"\"").unwrap();
        let result = parse_tilde_front(&input);
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
    fn word_parse_tilde_front_interrupted_by_non_literal() {
        let input = Word::from_str(r"~foo\/").unwrap();
        let result = parse_tilde_front(&input);
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
        let result = parse_tilde_front(&input);
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

    #[test]
    fn word_parse_tilde_everywhere_not_starting_with_tilde() {
        let input = Word::from_str("").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result, input);

        let input = Word::from_str("a").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result, input);

        let input = Word::from_str("''").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn word_parse_tilde_everywhere_only_tilde() {
        let input = Word::from_str("~").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(result.units, [Tilde("".to_string())]);
    }

    #[test]
    fn word_parse_tilde_everywhere_with_name() {
        let input = Word::from_str("~foo").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(result.units, [Tilde("foo".to_string())]);
    }

    #[test]
    fn word_parse_tilde_everywhere_ending_with_slash() {
        let input = Word::from_str("~bar/''").unwrap();
        let result = parse_tilde_everywhere(&input);
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
    fn word_parse_tilde_everywhere_ending_with_colon() {
        let input = Word::from_str("~bar:\"\"").unwrap();
        let result = parse_tilde_everywhere(&input);
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
    fn word_parse_tilde_everywhere_interrupted_by_non_literal() {
        let input = Word::from_str(r"~foo\/").unwrap();
        let result = parse_tilde_everywhere(&input);
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
        let result = parse_tilde_everywhere(&input);
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

    #[test]
    fn word_parse_tilde_everywhere_not_after_colon() {
        let input = Word::from_str("a~").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result, input);

        let input = Word::from_str("/~a").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result, input);

        let input = Word::from_str("''~/").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn word_parse_tilde_everywhere_after_colon() {
        let input = Word::from_str(":~").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [Unquoted(Literal(':')), Tilde("".to_string())]
        );

        let input = Word::from_str(":~foo/a:~bar").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Unquoted(Literal(':')),
                Tilde("foo".to_string()),
                Unquoted(Literal('/')),
                Unquoted(Literal('a')),
                Unquoted(Literal(':')),
                Tilde("bar".to_string()),
            ]
        );

        let input = Word::from_str("~a/b:~c/d").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde("a".to_string()),
                Unquoted(Literal('/')),
                Unquoted(Literal('b')),
                Unquoted(Literal(':')),
                Tilde("c".to_string()),
                Unquoted(Literal('/')),
                Unquoted(Literal('d')),
            ]
        );
    }
}
