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

//! Tilde expansion parser
//!
//! This module defines additional functions to parse tilde expansions in a word.

use crate::syntax::TextUnit::Literal;
use crate::syntax::Word;
use crate::syntax::WordUnit::{self, Tilde, Unquoted};

/// Parses a tilde expansion.
///
/// This function expects the first word unit to be an unquoted tilde character.
/// Following the tilde character, a sequence of unquoted literal characters is
/// parsed as the name of the tilde expansion. The sequence is terminated by a
/// slash character (or a colon character if `delimit_at_colon` is `true`).
///
/// If successful, this function returns a tuple of the length of the parsed
/// word units (including the tilde character), the name of the tilde
/// expansion (excluding the tilde character and the delimiter), and a Boolean
/// indicating whether the name is followed by a slash character.
/// Note that the name may be empty.
///
/// If the first word unit is not an unquoted tilde character or the name is
/// delimited by a word unit other than an unquoted literal character, this
/// function returns `None`.
fn parse_tilde<'a, I>(units: I, delimit_at_colon: bool) -> Option<(usize, String, bool)>
where
    I: IntoIterator<Item = &'a WordUnit>,
{
    let mut units = units.into_iter();
    if units.next() != Some(&Unquoted(Literal('~'))) {
        return None;
    }

    let mut name = String::new();
    let mut count = 1;

    for unit in units {
        match unit {
            Unquoted(Literal('/')) => return Some((count, name, true)),
            Unquoted(Literal(':')) if delimit_at_colon => break,
            Unquoted(Literal(c)) => {
                name.push(*c);
                count += 1;
            }
            _ => return None,
        }
    }

    Some((count, name, false))
}

impl Word {
    /// Parses a tilde expansion at the beginning of the word.
    ///
    /// This function checks if `self.units` begins with an unquoted tilde
    /// character, i.e., `WordUnit::Unquoted(TextUnit::Literal('~'))`. If so, the
    /// word unit is replaced with a `WordUnit::Tilde` value. Other unquoted
    /// characters that follow the tilde are together replaced to produce the
    /// value of the `WordUnit::Tilde`.
    ///
    /// ```
    /// # use std::str::FromStr;
    /// # use yash_syntax::syntax::{Word, WordUnit::Tilde};
    /// let mut word = Word::from_str("~").unwrap();
    /// word.parse_tilde_front();
    /// assert_eq!(word.units, [Tilde { name: "".to_string(), followed_by_slash: false }]);
    /// ```
    ///
    /// ```
    /// # use std::str::FromStr;
    /// # use yash_syntax::syntax::{Word, WordUnit::Tilde};
    /// let mut word = Word::from_str("~foo").unwrap();
    /// word.parse_tilde_front();
    /// assert_eq!(word.units, [Tilde { name: "foo".to_string(), followed_by_slash: false }]);
    /// ```
    ///
    /// If there is no leading tilde, `self.units` will have the same content
    /// when this function returns.
    ///
    /// ```
    /// # use std::str::FromStr;
    /// # use yash_syntax::syntax::{TextUnit::Literal, Word, WordUnit::Unquoted};
    /// let mut word = Word::from_str("X").unwrap();
    /// assert_eq!(word.units, [Unquoted(Literal('X'))]);
    /// word.parse_tilde_front();
    /// assert_eq!(word.units, [Unquoted(Literal('X'))]);
    /// ```
    ///
    /// This function parses a literal word units only, which differs from the
    /// strictly POSIX-conforming behavior. For example, POSIX requires the word
    /// `~$()` to be regarded as a tilde expansion, but this function does not
    /// convert it to `WordUnit::Tilde("$()".to_string())`.
    ///
    /// This function only parses a tilde expansion at the beginning of the word.
    /// If the word is a colon-separated list of paths, you might want to use
    /// [`parse_tilde_everywhere`](Self::parse_tilde_everywhere) instead.
    ///
    /// The tilde expansion is delimited by an unquoted slash. Unlike
    /// `parse_tilde_everywhere`, unquoted colons are not considered as
    /// delimiters.
    #[inline]
    pub fn parse_tilde_front(&mut self) {
        if let Some((len, name, followed_by_slash)) = parse_tilde(&self.units, false) {
            self.units.splice(
                ..len,
                std::iter::once(Tilde {
                    name,
                    followed_by_slash,
                }),
            );
        }
    }

    /// Parses tilde expansions in the word.
    ///
    /// This function works the same as
    /// [`parse_tilde_front`](Self::parse_tilde_front) except that it parses
    /// tilde expansions not only at the beginning of the word but also after
    /// each unquoted colon.
    ///
    /// ```
    /// # use std::str::FromStr;
    /// # use yash_syntax::syntax::{TextUnit::Literal, Word, WordUnit::{Tilde, Unquoted}};
    /// let mut word = Word::from_str("~:~a/b:~c").unwrap();
    /// word.parse_tilde_everywhere();
    /// assert_eq!(
    ///     word.units,
    ///     [
    ///         Tilde { name: "".to_string(), followed_by_slash: false },
    ///         Unquoted(Literal(':')),
    ///         Tilde { name: "a".to_string(), followed_by_slash: true },
    ///         Unquoted(Literal('/')),
    ///         Unquoted(Literal('b')),
    ///         Unquoted(Literal(':')),
    ///         Tilde { name: "c".to_string(), followed_by_slash: false },
    ///     ]
    /// );
    /// ```
    ///
    /// See also
    /// [`parse_tilde_everywhere_after`](Self::parse_tilde_everywhere_after),
    /// which allows you to parse tilde expansions only after a specified index.
    #[inline]
    pub fn parse_tilde_everywhere(&mut self) {
        self.parse_tilde_everywhere_after(0);
    }

    /// Parses tilde expansions in the word after the specified index.
    ///
    /// This function works the same as
    /// [`parse_tilde_everywhere`](Self::parse_tilde_everywhere) except that it
    /// starts parsing tilde expansions after the specified index of
    /// `self.units`. Tilde expansions are parsed at the specified index and
    /// after each unquoted colon.
    ///
    /// ```
    /// # use std::str::FromStr;
    /// # use yash_syntax::syntax::{TextUnit::Literal, Word, WordUnit::{Tilde, Unquoted}};
    /// let mut word = Word::from_str("~=~a/b:~c").unwrap();
    /// word.parse_tilde_everywhere_after(2);
    /// assert_eq!(
    ///     word.units,
    ///     [
    ///         // The initial tilde is not parsed because it is before index 2.
    ///         Unquoted(Literal('~')),
    ///         Unquoted(Literal('=')),
    ///         // This tilde is parsed because it is at index 2,
    ///         // even though it is not after a colon.
    ///         Tilde { name: "a".to_string(), followed_by_slash: true },
    ///         Unquoted(Literal('/')),
    ///         Unquoted(Literal('b')),
    ///         Unquoted(Literal(':')),
    ///         Tilde { name: "c".to_string(), followed_by_slash: false },
    ///     ]
    /// );
    /// ```
    ///
    /// Compare [`parse_tilde_everywhere`](Self::parse_tilde_everywhere), which
    /// is equivalent to `parse_tilde_everywhere_after(0)`.
    pub fn parse_tilde_everywhere_after(&mut self, index: usize) {
        let mut i = index;
        loop {
            // Parse a tilde expansion at index `i`.
            if let Some((len, name, followed_by_slash)) = parse_tilde(&self.units[i..], true) {
                self.units.splice(
                    i..i + len,
                    std::iter::once(Tilde {
                        name,
                        followed_by_slash,
                    }),
                );
                i += 1;
            }

            // Find the next colon separator.
            let Some(colon) = self.units[i..]
                .iter()
                .position(|unit| unit == &Unquoted(Literal(':')))
            else {
                break;
            };
            i += colon + 1;
        }
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
        assert_eq!(
            result.units,
            [Tilde {
                name: "".to_string(),
                followed_by_slash: false
            }]
        );
    }

    #[test]
    fn word_parse_tilde_front_with_name() {
        let input = Word::from_str("~foo").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [Tilde {
                name: "foo".to_string(),
                followed_by_slash: false
            }]
        );
    }

    #[test]
    fn word_parse_tilde_front_ending_with_slash() {
        let input = Word::from_str("~bar/''").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde {
                    name: "bar".to_string(),
                    followed_by_slash: true,
                },
                Unquoted(Literal('/')),
                SingleQuote("".to_string()),
            ]
        );
    }

    #[test]
    fn word_parse_tilde_front_including_colon() {
        let input = Word::from_str("~bar:baz").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [Tilde {
                name: "bar:baz".to_string(),
                followed_by_slash: false
            }]
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
    fn word_parse_tilde_front_not_after_colon() {
        let input = Word::from_str("a~").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result, input);

        let input = Word::from_str("/~a").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result, input);

        let input = Word::from_str("''~/").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result, input);
    }

    #[test]
    fn word_parse_tilde_front_after_colon() {
        let input = Word::from_str(":~").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [Unquoted(Literal(':')), Unquoted(Literal('~'))]
        );

        let input = Word::from_str(":~foo/a:~bar").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Unquoted(Literal(':')),
                Unquoted(Literal('~')),
                Unquoted(Literal('f')),
                Unquoted(Literal('o')),
                Unquoted(Literal('o')),
                Unquoted(Literal('/')),
                Unquoted(Literal('a')),
                Unquoted(Literal(':')),
                Unquoted(Literal('~')),
                Unquoted(Literal('b')),
                Unquoted(Literal('a')),
                Unquoted(Literal('r')),
            ]
        );

        let input = Word::from_str("~a/b:~c/d").unwrap();
        let result = parse_tilde_front(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde {
                    name: "a".to_string(),
                    followed_by_slash: true,
                },
                Unquoted(Literal('/')),
                Unquoted(Literal('b')),
                Unquoted(Literal(':')),
                Unquoted(Literal('~')),
                Unquoted(Literal('c')),
                Unquoted(Literal('/')),
                Unquoted(Literal('d')),
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
        assert_eq!(
            result.units,
            [Tilde {
                name: "".to_string(),
                followed_by_slash: false
            }]
        );
    }

    #[test]
    fn word_parse_tilde_everywhere_with_name() {
        let input = Word::from_str("~foo").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [Tilde {
                name: "foo".to_string(),
                followed_by_slash: false
            }]
        );
    }

    #[test]
    fn word_parse_tilde_everywhere_ending_with_slash() {
        let input = Word::from_str("~bar/''").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde {
                    name: "bar".to_string(),
                    followed_by_slash: true
                },
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
                Tilde {
                    name: "bar".to_string(),
                    followed_by_slash: false
                },
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
            [
                Unquoted(Literal(':')),
                Tilde {
                    name: "".to_string(),
                    followed_by_slash: false
                }
            ]
        );

        let input = Word::from_str(":~foo/a:~bar").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Unquoted(Literal(':')),
                Tilde {
                    name: "foo".to_string(),
                    followed_by_slash: true,
                },
                Unquoted(Literal('/')),
                Unquoted(Literal('a')),
                Unquoted(Literal(':')),
                Tilde {
                    name: "bar".to_string(),
                    followed_by_slash: false
                },
            ]
        );

        let input = Word::from_str("~a/b:~c/d::~").unwrap();
        let result = parse_tilde_everywhere(&input);
        assert_eq!(result.location, input.location);
        assert_eq!(
            result.units,
            [
                Tilde {
                    name: "a".to_string(),
                    followed_by_slash: true,
                },
                Unquoted(Literal('/')),
                Unquoted(Literal('b')),
                Unquoted(Literal(':')),
                Tilde {
                    name: "c".to_string(),
                    followed_by_slash: true,
                },
                Unquoted(Literal('/')),
                Unquoted(Literal('d')),
                Unquoted(Literal(':')),
                Unquoted(Literal(':')),
                Tilde {
                    name: "".to_string(),
                    followed_by_slash: false
                },
            ]
        );
    }
}
