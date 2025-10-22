// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Extracting ranges of split fields

use super::super::attr::AttrChar;
use super::ifs::Class::*;
use super::ifs::Ifs;
use std::iter::FusedIterator;
use std::ops::Range;

/// State of a field-splitting iterator
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum State {
    Midfield {
        start_index: usize,
    },

    AfterIfsWhitespace,

    #[default]
    AfterIfsNonWhitespace,
}

use State::*;

/// Iterator that yields index ranges of separated fields
///
/// This iterator can be created with [`Ifs::ranges`] and is used by
/// [`split_into`](super::split_into).
#[derive(Clone, Debug)]
pub struct Ranges<'a, I: Iterator<Item = AttrChar>> {
    inner: I,
    next_index: usize,
    ifs: &'a Ifs<'a>,
    state: Option<State>,
}

impl<'a> Ifs<'a> {
    /// Creates a field-splitting iterator.
    pub fn ranges<I>(&'a self, field_chars: I) -> Ranges<'a, I::IntoIter>
    where
        I: IntoIterator<Item = AttrChar>,
    {
        Ranges {
            inner: field_chars.into_iter(),
            next_index: 0,
            ifs: self,
            state: Some(State::default()),
        }
    }
}

impl<I> Iterator for Ranges<'_, I>
where
    I: Iterator<Item = AttrChar>,
{
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Range<usize>> {
        while let Some(state) = self.state {
            let index = self.next_index;
            let class = self.inner.next().map(|c| self.ifs.classify_attr(c));
            self.next_index += 1;

            let (next_state, field_range) = match (state, class) {
                (Midfield { start_index }, Some(IfsNonWhitespace) | None) => {
                    (Some(AfterIfsNonWhitespace), Some(start_index..index))
                }
                (Midfield { start_index }, Some(IfsWhitespace)) => {
                    (Some(AfterIfsWhitespace), Some(start_index..index))
                }
                (Midfield { .. }, Some(NonIfs)) => (Some(state), None),
                (AfterIfsWhitespace, Some(IfsNonWhitespace)) => (Some(AfterIfsNonWhitespace), None),
                (AfterIfsNonWhitespace, Some(IfsNonWhitespace)) => {
                    (Some(state), Some(index..index))
                }
                (_, Some(NonIfs)) => (Some(Midfield { start_index: index }), None),
                (_, Some(IfsWhitespace)) => (Some(state), None),
                (_, None) => (None, None),
            };

            self.state = next_state;
            if field_range.is_some() {
                return field_range;
            }
        }

        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, mut upper_bound) = self.inner.size_hint();

        if self.ifs.chars().is_empty() {
            // No splitting performed. The result will be no more than one field.
            if upper_bound != Some(0) {
                upper_bound = Some(1);
            }
        } else if self.ifs.non_whitespaces().is_empty() {
            // All separators are whitespace. An alternating sequence of
            // separators and non-separators will produce the most fields.
            if let Some(ref mut upper_bound) = upper_bound {
                // We can't do this because of possible overflow:
                // *upper_bound = (*upper_bound + 1) / 2;
                if *upper_bound > 0 {
                    *upper_bound = (*upper_bound - 1) / 2 + 1;
                }
            }
        } else {
            // The field may contain non-whitespace separators. When all the
            // input characters are separators, there will be as many fields as
            // the separators.
            // TODO When the last-empty-field option applies, there may be one more field.
            // upper_bound = upper_bound.and_then(|ub| ub.checked_add(1));
        }

        (0, upper_bound)
    }
}

impl<I> FusedIterator for Ranges<'_, I> where I: Iterator<Item = AttrChar> {}

#[allow(clippy::single_range_in_vec_init)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::expansion::attr::Origin;

    fn attr_chars(s: &str) -> impl Iterator<Item = AttrChar> + '_ {
        s.chars().map(|c| AttrChar {
            value: c,
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        })
    }

    #[test]
    fn empty_input() {
        let ifs = Ifs::default();
        let ranges = ifs.ranges([]).collect::<Vec<_>>();
        assert_eq!(ranges, [] as [Range<usize>; 0]);
    }

    #[test]
    fn input_containing_whitespace_separators_only() {
        let ifs = Ifs::default();
        let ranges = ifs.ranges(attr_chars(" \n\t")).collect::<Vec<_>>();
        assert_eq!(ranges, [] as [Range<usize>; 0]);
    }

    #[test]
    fn input_containing_non_whitespace_separators_only() {
        let ifs = Ifs::new("-");

        let ranges = ifs.ranges(attr_chars("")).collect::<Vec<_>>();
        assert_eq!(ranges, [] as [Range<usize>; 0]);

        let ranges = ifs.ranges(attr_chars("-")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0]);

        let ranges = ifs.ranges(attr_chars("--")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0, 1..1]);

        let ranges = ifs.ranges(attr_chars("---")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0, 1..1, 2..2]);
    }

    #[test]
    fn input_containing_one_field_only() {
        let ifs = Ifs::default();

        let ranges = ifs.ranges(attr_chars("-")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1]);

        let ranges = ifs.ranges(attr_chars("--")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..2]);

        let ranges = ifs.ranges(attr_chars("---")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..3]);
    }

    #[test]
    fn fields_separated_by_non_whitespaces() {
        let ifs = Ifs::new("-");

        let ranges = ifs.ranges(attr_chars("a-")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1]);

        let ranges = ifs.ranges(attr_chars("a-a")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1, 2..3]);

        let ranges = ifs.ranges(attr_chars("-a-")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0, 1..2]);

        let ranges = ifs.ranges(attr_chars("a-aa--aaa")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1, 2..4, 5..5, 6..9]);

        let ranges = ifs.ranges(attr_chars("---aa--a-")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0, 1..1, 2..2, 3..5, 6..6, 7..8]);
    }

    #[test]
    fn fields_separated_by_whitespaces() {
        let ifs = Ifs::default();

        let ranges = ifs.ranges(attr_chars("a ")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1]);

        let ranges = ifs.ranges(attr_chars("a a")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1, 2..3]);

        let ranges = ifs.ranges(attr_chars(" a ")).collect::<Vec<_>>();
        assert_eq!(ranges, [1..2]);

        let ranges = ifs.ranges(attr_chars("a aa  aaa")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1, 2..4, 6..9]);

        let ranges = ifs.ranges(attr_chars("   aa  a ")).collect::<Vec<_>>();
        assert_eq!(ranges, [3..5, 7..8]);
    }

    #[test]
    fn ifs_whitespace_followed_by_ifs_non_whitespace() {
        let ifs = Ifs::new(" -");

        let ranges = ifs.ranges(attr_chars("a -")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..1]);

        let ranges = ifs.ranges(attr_chars("aa  -a   - -")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..2, 5..6, 11..11]);
    }

    #[test]
    fn ifs_non_whitespace_followed_by_ifs_whitespace() {
        let ifs = Ifs::new(" -");

        let ranges = ifs.ranges(attr_chars("- ")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0]);

        let ranges = ifs.ranges(attr_chars("--  ")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0, 1..1]);

        let ranges = ifs.ranges(attr_chars("-  -  aa")).collect::<Vec<_>>();
        assert_eq!(ranges, [0..0, 3..3, 6..8]);
    }

    #[test]
    fn quoted_chars_are_not_separators() {
        fn quoted(value: char, is_quoted: bool) -> AttrChar {
            AttrChar {
                value,
                origin: Origin::SoftExpansion,
                is_quoted,
                is_quoting: false,
            }
        }

        let ifs = Ifs::new(" -");
        let ranges = ifs
            .ranges([
                quoted(' ', false),
                quoted('-', false),
                quoted(' ', true),
                quoted('-', true),
                quoted(' ', false),
                quoted('-', false),
            ])
            .collect::<Vec<_>>();
        assert_eq!(ranges, [1..1, 2..4]);
    }

    #[test]
    fn quoting_chars_are_not_separators() {
        fn quoting(value: char, is_quoting: bool) -> AttrChar {
            AttrChar {
                value,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting,
            }
        }

        let ifs = Ifs::new(" -");
        let ranges = ifs
            .ranges([
                quoting(' ', false),
                quoting('-', false),
                quoting(' ', true),
                quoting('-', true),
                quoting(' ', false),
                quoting('-', false),
            ])
            .collect::<Vec<_>>();
        assert_eq!(ranges, [1..1, 2..4]);
    }

    #[test]
    fn only_soft_expansion_chars_are_split() {
        fn with_origin(value: char, origin: Origin) -> AttrChar {
            AttrChar {
                value,
                origin,
                is_quoted: false,
                is_quoting: false,
            }
        }

        let ifs = Ifs::new(" -");
        let ranges = ifs
            .ranges([
                with_origin(' ', Origin::SoftExpansion),
                with_origin('-', Origin::SoftExpansion),
                with_origin(' ', Origin::Literal),
                with_origin('-', Origin::Literal),
                with_origin('-', Origin::HardExpansion),
                with_origin(' ', Origin::SoftExpansion),
                with_origin('-', Origin::SoftExpansion),
            ])
            .collect::<Vec<_>>();
        assert_eq!(ranges, [1..1, 2..5]);
    }
}
