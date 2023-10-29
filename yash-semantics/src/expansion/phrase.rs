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

//! Array of fields as an intermediate expansion result
//!
//! This module defines [`Phrase`], a data type for holding intermediate
//! expansion results. A phrase is an array of (optional, possibly zero) fields,
//! and a field is a string of attributed characters ([`AttrChar`]).
//!
//! The most general form of a phrase is represented by `Vec<Vec<AttrChar>>`.
//! However, many expansion results in a phrase of only one field. The `Phrase`
//! type can represent a single-field phrase by only holding a `Vec<AttrChar>`
//! or `AttrChar` instance to reduce memory allocation overheads.
//!
//! You can join two or more phrases into a single by using the
//! [`append`](Phrase::append) method and the `+` and `+=` operators. Phrase
//! concatenation is not the same as appending a `Vec<Vec<AttrChar>>` to
//! another. When phrases are joined, the last field of the first phrase and the
//! first field of the second phrase are concatenated while other fields remain
//! intact.

use crate::expansion::attr::AttrChar;
use crate::expansion::attr::Origin;
use std::iter::FusedIterator;
use std::ops::Add;
use std::ops::AddAssign;
use yash_env::variable::Value;
use yash_env::variable::VariableSet;

/// Array of fields with optimized data structure
///
/// See the [module documentation](self).
#[derive(Clone, Debug, Eq)]
pub enum Phrase {
    /// Phrase having one field containing one character
    ///
    /// `Phrase::Char(c)` is equivalent to `Phrase::Field(vec![c])`, which in
    /// turn equals `Phrase::Full(vec![vec![c]])`.
    Char(AttrChar),
    /// Phrase made up of one field
    ///
    /// `Phrase::Field(chars)` is equivalent to `Phrase::Full(vec![chars])`.
    Field(Vec<AttrChar>),
    /// Phrase containing any number of fields
    Full(Vec<Vec<AttrChar>>),
}

use Phrase::*;

impl PartialEq for Phrase {
    #[must_use]
    fn eq(&self, other: &Phrase) -> bool {
        match (self, other) {
            (Char(left), Char(right)) => left == right,
            (Field(left), Field(right)) => left == right,
            (Full(left), Full(right)) => left == right,
            (Char(c), Field(f)) | (Field(f), Char(c)) => [*c].as_slice() == f.as_slice(),
            (Char(c), Full(v)) | (Full(v), Char(c)) => {
                matches!(v.as_slice(), [f] if [*c].as_slice() == f.as_slice())
            }
            (Field(f), Full(v)) | (Full(v), Field(f)) => {
                matches!(v.as_slice(), [fv] if f == fv)
            }
        }
    }
}

impl Phrase {
    /// Returns a phrase containing no fields.
    ///
    /// This function requires no heap allocation.
    #[inline]
    #[must_use]
    pub fn zero_fields() -> Self {
        Full(Vec::new())
    }

    /// Returns a phrase containing one empty field.
    ///
    /// This function requires no heap allocation.
    #[inline]
    #[must_use]
    pub fn one_empty_field() -> Self {
        Field(Vec::new())
    }

    /// Tests whether the phrase has no fields.
    #[must_use]
    pub fn is_zero_fields(&self) -> bool {
        matches!(self, Full(fields) if fields.is_empty())
    }

    /// Returns the number of fields in the phrase.
    #[must_use]
    pub fn field_count(&self) -> usize {
        match self {
            Char(_) | Field(_) => 1,
            Full(fields) => fields.len(),
        }
    }

    /// Moves all fields of `other` into `self`, leaving `other` empty.
    ///
    /// This function joins two phrases into one. It concatenates the last field
    /// of `self` and the first field of `other`. Other fields are left intact
    /// in the new phrase.
    ///
    /// ```
    /// # use yash_semantics::expansion::{attr::{AttrChar, Origin}, phrase::Phrase};
    /// # let a = AttrChar {
    /// #     value: 'a',
    /// #     origin: Origin::Literal,
    /// #     is_quoted: false,
    /// #     is_quoting: false,
    /// # };
    /// # let b = AttrChar { value: 'b', ..a };
    /// # let c = AttrChar { value: 'c', ..a };
    /// # let d = AttrChar { value: 'd', ..a };
    /// # let e = AttrChar { value: 'e', ..a };
    /// # let f = AttrChar { value: 'f', ..a };
    /// let mut left = Phrase::Full(vec![vec![a], vec![b], vec![c]]);
    /// let mut right = Phrase::Full(vec![vec![d], vec![e], vec![f]]);
    /// left.append(&mut right);
    /// assert_eq!(
    ///     left,
    ///     Phrase::Full(vec![vec![a], vec![b], vec![c, d], vec![e], vec![f]])
    /// );
    /// assert!(right.is_zero_fields());
    /// ```
    ///
    /// That implies joining two single-field phrases results in another
    /// single-field phrase.
    ///
    /// ```
    /// # use yash_semantics::expansion::{attr::{AttrChar, Origin}, phrase::Phrase};
    /// # let a = AttrChar {
    /// #     value: 'a',
    /// #     origin: Origin::Literal,
    /// #     is_quoted: false,
    /// #     is_quoting: false,
    /// # };
    /// # let b = AttrChar { value: 'b', ..a };
    /// # let c = AttrChar { value: 'c', ..a };
    /// # let d = AttrChar { value: 'd', ..a };
    /// # let e = AttrChar { value: 'e', ..a };
    /// # let f = AttrChar { value: 'f', ..a };
    /// let mut left = Phrase::Field(vec![a, b, c]);
    /// let mut right = Phrase::Field(vec![d, e, f]);
    /// left.append(&mut right);
    /// assert_eq!(left, Phrase::Field(vec![a, b, c, d, e, f]));
    /// assert!(right.is_zero_fields());
    /// ```
    ///
    /// ```
    /// # use yash_semantics::expansion::{attr::{AttrChar, Origin}, phrase::Phrase};
    /// # let a = AttrChar {
    /// #     value: 'a',
    /// #     origin: Origin::Literal,
    /// #     is_quoted: false,
    /// #     is_quoting: false,
    /// # };
    /// # let b = AttrChar { value: 'b', ..a };
    /// let mut left = Phrase::Char(a);
    /// let mut right = Phrase::Char(b);
    /// left.append(&mut right);
    /// assert_eq!(left, Phrase::Field(vec![a, b]));
    /// assert!(right.is_zero_fields());
    /// ```
    ///
    /// ```
    /// # use yash_semantics::expansion::phrase::Phrase;
    /// let mut left = Phrase::one_empty_field();
    /// let mut right = Phrase::one_empty_field();
    /// left.append(&mut right);
    /// assert_eq!(left, Phrase::one_empty_field());
    /// assert_eq!(right, Phrase::zero_fields());
    /// ```
    ///
    /// If either phrase is zero fields, the result is the other.
    pub fn append(&mut self, other: &mut Phrase) {
        match (&mut *self, &mut *other) {
            (Char(left), Char(right)) => {
                *self = Field(vec![*left, *right]);
                *other = Phrase::zero_fields();
            }
            (Char(left), Field(right)) => {
                right.insert(0, *left);
                *self = std::mem::replace(other, Phrase::zero_fields());
            }
            (Field(left), Char(right)) => {
                left.push(*right);
                *other = Phrase::zero_fields();
            }
            (Field(left), Field(right)) => {
                left.append(right);
                *other = Phrase::zero_fields();
            }
            (left, Full(right)) => {
                if let Some(right_first) = right.first_mut() {
                    match left {
                        Char(left) => {
                            right_first.insert(0, *left);
                            *self = std::mem::replace(other, Phrase::zero_fields());
                        }
                        Field(left) => {
                            left.append(right_first);
                            std::mem::swap(left, right_first);
                            *self = std::mem::replace(other, Phrase::zero_fields());
                        }
                        Full(left) => {
                            if let Some(left_last) = left.last_mut() {
                                left_last.append(right_first);
                                left.extend(right.drain(1..));
                                right.clear();
                            } else {
                                std::mem::swap(left, right);
                            }
                        }
                    }
                } else {
                    // Nothing to do
                }
            }
            (Full(left), right) => {
                if let Some(left_last) = left.last_mut() {
                    match right {
                        Char(right) => left_last.push(*right),
                        Field(right) => left_last.append(right),
                        Full(_right) => unreachable!(),
                    }
                    *other = Phrase::zero_fields();
                } else {
                    std::mem::swap(self, other);
                }
            }
        }
    }

    /// Joins this phrase into a single field, separated by the first IFS character.
    ///
    /// This function joins `self` into a single field, separating each original
    /// field by the first character of variable `IFS`. If the variable is not
    /// set, fields are separated by a space. If the variable is set but has an
    /// empty value, fields are joined without separation.
    pub fn ifs_join(self, vars: &VariableSet) -> Vec<AttrChar> {
        match self {
            Char(c) => vec![c],
            Field(field) => field,
            Full(mut fields) => match fields.len() {
                0 => vec![],
                1 => fields.swap_remove(0),
                _ => {
                    let separator = match vars.get("IFS").and_then(|v| v.value.as_ref()) {
                        Some(Value::Scalar(value)) => value.chars().next(),
                        Some(Value::Array(values)) => {
                            values.first().and_then(|value| value.chars().next())
                        }
                        None => Some(' '),
                    }
                    .map(|c| AttrChar {
                        value: c,
                        origin: Origin::SoftExpansion,
                        is_quoted: false,
                        is_quoting: false,
                    });

                    let mut i = fields.into_iter();
                    let mut result = i.next().unwrap();
                    result.reserve_exact(
                        i.as_slice().iter().map(|field| field.len()).sum::<usize>()
                            + i.as_slice().len(),
                    );
                    for field in i {
                        if let Some(separator) = separator {
                            result.push(separator);
                        }
                        result.extend(field);
                    }
                    result
                }
            },
        }
    }

    /// Applies a function to every character in the phrase.
    pub fn for_each_char_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut AttrChar),
    {
        match self {
            Char(c) => f(c),
            Field(field) => field.iter_mut().for_each(f),
            Full(fields) => fields.iter_mut().flatten().for_each(f),
        }
    }
}

impl From<AttrChar> for Phrase {
    #[inline]
    #[must_use]
    fn from(c: AttrChar) -> Self {
        Char(c)
    }
}

impl From<Vec<AttrChar>> for Phrase {
    #[inline]
    #[must_use]
    fn from(chars: Vec<AttrChar>) -> Self {
        Field(chars)
    }
}

impl From<Vec<Vec<AttrChar>>> for Phrase {
    #[inline]
    #[must_use]
    fn from(fields: Vec<Vec<AttrChar>>) -> Self {
        Full(fields)
    }
}

impl From<Phrase> for Vec<Vec<AttrChar>> {
    #[must_use]
    fn from(phrase: Phrase) -> Self {
        match phrase {
            Char(c) => vec![vec![c]],
            Field(f) => vec![f],
            Full(v) => v,
        }
    }
}

/// Private implementation detail of [`IntoIter`]
#[derive(Clone, Debug)]
enum IntoIterState {
    None,
    Char(AttrChar),
    Field(Vec<AttrChar>),
    Full(std::vec::IntoIter<Vec<AttrChar>>),
}

/// Iterator of fields
///
/// You can turn a [`Phrase`] into an iterator by
/// [`into_iter`](Phrase::into_iter).
#[derive(Clone, Debug)]
pub struct IntoIter(IntoIterState);

impl Iterator for IntoIter {
    type Item = Vec<AttrChar>;
    fn next(&mut self) -> Option<Vec<AttrChar>> {
        match &mut self.0 {
            IntoIterState::None => None,
            IntoIterState::Char(c) => {
                let f = vec![*c];
                self.0 = IntoIterState::None;
                Some(f)
            }
            IntoIterState::Field(f) => {
                let f = std::mem::take(f);
                self.0 = IntoIterState::None;
                Some(f)
            }
            IntoIterState::Full(i) => i.next(),
        }
    }

    #[must_use]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            IntoIterState::None => (0, Some(0)),
            IntoIterState::Char(_) | IntoIterState::Field(_) => (1, Some(1)),
            IntoIterState::Full(i) => i.size_hint(),
        }
    }

    #[inline]
    #[must_use]
    fn count(self) -> usize {
        self.len()
    }
}

impl DoubleEndedIterator for IntoIter {
    fn next_back(&mut self) -> Option<Vec<AttrChar>> {
        match &mut self.0 {
            IntoIterState::None => None,
            IntoIterState::Char(c) => {
                let f = vec![*c];
                self.0 = IntoIterState::None;
                Some(f)
            }
            IntoIterState::Field(f) => {
                let f = std::mem::take(f);
                self.0 = IntoIterState::None;
                Some(f)
            }
            IntoIterState::Full(i) => i.next_back(),
        }
    }
}

impl ExactSizeIterator for IntoIter {}

impl FusedIterator for IntoIter {}

impl IntoIterator for Phrase {
    type Item = Vec<AttrChar>;
    type IntoIter = IntoIter;
    #[must_use]
    fn into_iter(self) -> IntoIter {
        IntoIter(match self {
            Char(c) => IntoIterState::Char(c),
            Field(f) => IntoIterState::Field(f),
            Full(f) => IntoIterState::Full(f.into_iter()),
        })
    }
}

// We do not implement FromIterator or Extend for Phrase because their behavior
// would be confusing compared with that of AddAssign and Add.
// You should directly manipulate Vec<Vec<AttrChar>>.

/// See [`Phrase::append`].
impl AddAssign for Phrase {
    fn add_assign(&mut self, mut other: Phrase) {
        self.append(&mut other)
    }
}

/// See [`Phrase::append`].
impl Add for Phrase {
    type Output = Phrase;
    #[inline]
    #[must_use]
    fn add(mut self, other: Phrase) -> Self {
        self.add_assign(other);
        self
    }
}

// We do not implement std::iter::Sum for Phrase because it is not obvious
// what the sum of an empty iterator should be.

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::variable::Scope;

    #[test]
    fn partial_eq() {
        let c1 = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let c2 = AttrChar { value: 'b', ..c1 };
        let f0 = vec![];
        let f1 = vec![c1];
        let f2 = vec![c2];
        let f3 = vec![c1, c2];
        let ve = vec![];
        let v0 = vec![f0.clone()];
        let v1 = vec![f1.clone()];
        let v2 = vec![f2.clone()];
        let v3 = vec![f3.clone()];
        let v4 = vec![f1.clone(), f2.clone()];

        let c1 = Char(c1);
        let c2 = Char(c2);
        let f0 = Field(f0);
        let f1 = Field(f1);
        let f2 = Field(f2);
        let f3 = Field(f3);
        let ve = Full(ve);
        let v0 = Full(v0);
        let v1 = Full(v1);
        let v2 = Full(v2);
        let v3 = Full(v3);
        let v4 = Full(v4);

        assert_eq!(c1, c1);
        assert_eq!(c2, c2);
        assert_ne!(c1, c2);
        assert_ne!(c2, c1);

        assert_eq!(f0, f0);
        assert_eq!(f1, f1);
        assert_eq!(f3, f3);
        assert_ne!(f0, f1);
        assert_ne!(f1, f2);
        assert_ne!(f2, f3);
        assert_ne!(f3, f0);

        assert_eq!(ve, ve);
        assert_eq!(v2, v2);
        assert_eq!(v4, v4);
        assert_ne!(ve, v0);
        assert_ne!(v0, v1);
        assert_ne!(v1, v2);
        assert_ne!(v2, v3);
        assert_ne!(v3, v4);
        assert_ne!(v4, ve);

        assert_eq!(c1, f1);
        assert_eq!(c2, f2);
        assert_ne!(c1, f2);
        assert_ne!(c2, f3);
        assert_ne!(c2, f0);

        assert_eq!(f1, c1);
        assert_eq!(f2, c2);
        assert_ne!(f2, c1);
        assert_ne!(f3, c2);
        assert_ne!(f0, c2);

        assert_eq!(c1, v1);
        assert_eq!(c2, v2);
        assert_ne!(c1, v2);
        assert_ne!(c2, v3);
        assert_ne!(c1, v4);
        assert_ne!(c2, ve);

        assert_eq!(v1, c1);
        assert_eq!(v2, c2);
        assert_ne!(v2, c1);
        assert_ne!(v3, c2);
        assert_ne!(v4, c1);
        assert_ne!(ve, c2);

        assert_eq!(f0, v0);
        assert_eq!(f1, v1);
        assert_eq!(f2, v2);
        assert_eq!(f3, v3);
        assert_ne!(f0, ve);
        assert_ne!(f1, v0);
        assert_ne!(f2, v1);
        assert_ne!(f3, v2);
        assert_ne!(f3, v4);

        assert_eq!(v0, f0);
        assert_eq!(v1, f1);
        assert_eq!(v2, f2);
        assert_eq!(v3, f3);
        assert_ne!(ve, f0);
        assert_ne!(v0, f1);
        assert_ne!(v1, f2);
        assert_ne!(v2, f3);
        assert_ne!(v4, f3);
    }

    #[test]
    fn is_zero_fields() {
        let c = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };

        assert!(Full(vec![]).is_zero_fields());

        assert!(!Char(c).is_zero_fields());
        assert!(!Field(vec![]).is_zero_fields());
        assert!(!Field(vec![c]).is_zero_fields());
        assert!(!Field(vec![c, c]).is_zero_fields());
        assert!(!Full(vec![vec![]]).is_zero_fields());
        assert!(!Full(vec![vec![c]]).is_zero_fields());
        assert!(!Full(vec![vec![], vec![]]).is_zero_fields());
    }

    #[test]
    fn field_count() {
        let c = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(Char(c).field_count(), 1);
        assert_eq!(Field(vec![c]).field_count(), 1);
        assert_eq!(Phrase::zero_fields().field_count(), 0);
        assert_eq!(Phrase::one_empty_field().field_count(), 1);
        assert_eq!(Full(vec![vec![c]]).field_count(), 1);
        assert_eq!(Full(vec![vec![c, c]]).field_count(), 1);
        assert_eq!(Full(vec![vec![c], vec![c]]).field_count(), 2);
    }

    #[test]
    fn into_iter_size_hint() {
        let c = AttrChar {
            value: 'x',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };

        let mut i = Char(c).into_iter();
        assert_eq!(i.size_hint(), (1, Some(1)));
        i.next();
        assert_eq!(i.size_hint(), (0, Some(0)));

        let mut i = Field(vec![c, c]).into_iter();
        assert_eq!(i.size_hint(), (1, Some(1)));
        i.next();
        assert_eq!(i.size_hint(), (0, Some(0)));

        let mut i = Full(vec![vec![c], vec![c, c, c], vec![c, c]]).into_iter();
        assert_eq!(i.size_hint(), (3, Some(3)));
        i.next();
        assert_eq!(i.size_hint(), (2, Some(2)));
        i.next();
        assert_eq!(i.size_hint(), (1, Some(1)));
        i.next();
        assert_eq!(i.size_hint(), (0, Some(0)));
    }

    #[test]
    fn into_iter_count() {
        let c = AttrChar {
            value: 'x',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };

        let i = Char(c).into_iter();
        assert_eq!(i.count(), 1);

        let mut i = Char(c).into_iter();
        i.next();
        assert_eq!(i.count(), 0);

        let i = Field(vec![c, c]).into_iter();
        assert_eq!(i.count(), 1);

        let mut i = Field(vec![c, c]).into_iter();
        i.next();
        assert_eq!(i.count(), 0);

        let mut i = Full(vec![vec![c], vec![c, c, c], vec![c, c]]).into_iter();
        i.next();
        assert_eq!(i.count(), 2);

        let mut i = Full(vec![vec![c], vec![c, c, c], vec![c, c]]).into_iter();
        i.next();
        i.next();
        i.next();
        assert_eq!(i.count(), 0);
    }

    #[test]
    fn into_iter_fused() {
        let c = AttrChar {
            value: 'x',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };

        let mut i = Char(c).into_iter();
        assert_eq!(i.next(), Some(vec![c]));
        assert_eq!(i.next(), None);
        assert_eq!(i.next(), None);

        let mut i = Field(vec![c, c]).into_iter();
        assert_eq!(i.next(), Some(vec![c, c]));
        assert_eq!(i.next(), None);
        assert_eq!(i.next(), None);

        let mut i = Full(vec![vec![c], vec![c, c, c], vec![c, c]]).into_iter();
        assert_eq!(i.next(), Some(vec![c]));
        assert_eq!(i.next(), Some(vec![c, c, c]));
        assert_eq!(i.next(), Some(vec![c, c]));
        assert_eq!(i.next(), None);
        assert_eq!(i.next(), None);
    }

    #[test]
    fn into_iter_back_fused() {
        let c = AttrChar {
            value: 'x',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };

        let mut i = Char(c).into_iter();
        assert_eq!(i.next_back(), Some(vec![c]));
        assert_eq!(i.next_back(), None);
        assert_eq!(i.next_back(), None);

        let mut i = Field(vec![c, c]).into_iter();
        assert_eq!(i.next_back(), Some(vec![c, c]));
        assert_eq!(i.next_back(), None);
        assert_eq!(i.next_back(), None);

        let mut i = Full(vec![vec![c], vec![c, c, c], vec![c, c]]).into_iter();
        assert_eq!(i.next_back(), Some(vec![c, c]));
        assert_eq!(i.next_back(), Some(vec![c, c, c]));
        assert_eq!(i.next_back(), Some(vec![c]));
        assert_eq!(i.next_back(), None);
        assert_eq!(i.next_back(), None);
    }

    #[test]
    fn append_empty_empty() {
        let mut left = Phrase::zero_fields();
        let mut right = Phrase::zero_fields();
        left.append(&mut right);
        assert_eq!(left, Phrase::zero_fields());
        assert_eq!(right, Phrase::zero_fields());

        let mut left = Phrase::one_empty_field();
        let mut right = Phrase::zero_fields();
        left.append(&mut right);
        assert_eq!(left, Phrase::one_empty_field());
        assert_eq!(right, Phrase::zero_fields());

        let mut left = Phrase::zero_fields();
        let mut right = Phrase::one_empty_field();
        left.append(&mut right);
        assert_eq!(left, Phrase::one_empty_field());
        assert_eq!(right, Phrase::zero_fields());

        // See also doc test for Phrase::append
    }

    #[test]
    fn append_empty_char() {
        let phrase = Char(AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        });
        for mut left in [
            Phrase::zero_fields(),
            Phrase::one_empty_field(),
            Full(vec![]),
            Full(vec![vec![]]),
        ] {
            let mut right = phrase.clone();
            left.append(&mut right);
            assert_eq!(left, phrase);
            assert_eq!(right, Phrase::zero_fields());
        }
    }

    #[test]
    fn append_char_empty() {
        let phrase = Char(AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        });
        for mut right in [
            Phrase::zero_fields(),
            Phrase::one_empty_field(),
            Full(vec![]),
            Full(vec![vec![]]),
        ] {
            let mut left = phrase.clone();
            left.append(&mut right);
            assert_eq!(left, phrase);
            assert_eq!(right, Phrase::zero_fields());
        }
    }

    // See the doc test in Phrase::append
    // #[test]
    // fn append_char_char() {}

    #[test]
    fn append_empty_field() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let phrase = Field(vec![a, b, c]);
        for mut left in [
            Phrase::zero_fields(),
            Phrase::one_empty_field(),
            Full(vec![]),
            Full(vec![vec![]]),
        ] {
            let mut right = phrase.clone();
            left.append(&mut right);
            assert_eq!(left, phrase);
            assert_eq!(right, Phrase::zero_fields());
        }
    }

    #[test]
    fn append_field_empty() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let phrase = Field(vec![a, b, c]);
        for mut right in [
            Phrase::zero_fields(),
            Phrase::one_empty_field(),
            Full(vec![]),
            Full(vec![vec![]]),
        ] {
            let mut left = phrase.clone();
            left.append(&mut right);
            assert_eq!(left, phrase);
            assert_eq!(right, Phrase::zero_fields());
        }
    }

    #[test]
    fn append_char_field() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let mut left = Char(a);
        let mut right = Field(vec![b, c]);
        left.append(&mut right);
        assert_eq!(left, Field(vec![a, b, c]));
        assert_eq!(right, Phrase::zero_fields());
    }

    #[test]
    fn append_field_char() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let mut left = Field(vec![a, b]);
        let mut right = Char(c);
        left.append(&mut right);
        assert_eq!(left, Field(vec![a, b, c]));
        assert_eq!(right, Phrase::zero_fields());
    }

    // See the doc test in Phrase::append
    // #[test]
    // fn append_field_field() {}

    #[test]
    fn append_empty_full() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let phrase = Full(vec![vec![a], vec![b], vec![c]]);
        for mut left in [
            Phrase::zero_fields(),
            Phrase::one_empty_field(),
            Full(vec![]),
            Full(vec![vec![]]),
        ] {
            let mut right = phrase.clone();
            left.append(&mut right);
            assert_eq!(left, phrase);
            assert_eq!(right, Phrase::zero_fields());
        }
    }

    #[test]
    fn append_full_empty() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let phrase = Full(vec![vec![a], vec![b], vec![c]]);
        for mut right in [
            Phrase::zero_fields(),
            Phrase::one_empty_field(),
            Full(vec![]),
            Full(vec![vec![]]),
        ] {
            let mut left = phrase.clone();
            left.append(&mut right);
            assert_eq!(left, phrase);
            assert_eq!(right, Phrase::zero_fields());
        }
    }

    #[test]
    fn append_char_full() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let mut left = Char(a);
        let mut right = Full(vec![vec![b], vec![c]]);
        left.append(&mut right);
        assert_eq!(left, Full(vec![vec![a, b], vec![c]]));
        assert_eq!(right, Phrase::zero_fields());
    }

    #[test]
    fn append_full_char() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let mut left = Full(vec![vec![a], vec![b]]);
        let mut right = Char(c);
        left.append(&mut right);
        assert_eq!(left, Full(vec![vec![a], vec![b, c]]));
        assert_eq!(right, Phrase::zero_fields());
    }

    #[test]
    fn append_field_full() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let mut left = Field(vec![a]);
        let mut right = Full(vec![vec![b], vec![c]]);
        left.append(&mut right);
        assert_eq!(left, Full(vec![vec![a, b], vec![c]]));
        assert_eq!(right, Phrase::zero_fields());
    }

    #[test]
    fn append_full_field() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let b = AttrChar { value: 'b', ..a };
        let c = AttrChar { value: 'c', ..a };
        let mut left = Full(vec![vec![a], vec![b]]);
        let mut right = Field(vec![c]);
        left.append(&mut right);
        assert_eq!(left, Full(vec![vec![a], vec![b, c]]));
        assert_eq!(right, Phrase::zero_fields());
    }

    // See the doc test in Phrase::append
    // #[test]
    // fn append_full_full() {}

    fn dummy_field(chars: &str) -> Vec<AttrChar> {
        chars
            .chars()
            .map(|c| AttrChar {
                value: c,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            })
            .collect()
    }

    #[test]
    fn ifs_join_char() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let phrase = Char(a);
        let field = phrase.ifs_join(&VariableSet::new());
        assert_eq!(field, [a]);
    }

    #[test]
    fn ifs_join_field() {
        let field_in = dummy_field("abc");
        let phrase = Field(field_in.clone());
        let field_out = phrase.ifs_join(&VariableSet::new());
        assert_eq!(field_out, field_in);
    }

    #[test]
    fn ifs_join_full_empty() {
        let phrase = Full(vec![]);
        let field = phrase.ifs_join(&VariableSet::new());
        assert_eq!(field, []);
    }

    #[test]
    fn ifs_join_full_one() {
        let field_in = dummy_field("foo");
        let phrase = Full(vec![field_in.clone()]);
        let field_out = phrase.ifs_join(&VariableSet::new());
        assert_eq!(field_out, field_in);
    }

    #[test]
    fn ifs_join_full_unset_ifs() {
        let phrase = Full(vec![vec![], vec![]]);
        let field = phrase.ifs_join(&VariableSet::new());
        assert_eq!(field, dummy_field(" "));

        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&VariableSet::new());
        assert_eq!(field, dummy_field("foo bar baz"));
    }

    #[test]
    fn ifs_join_full_unassigned_ifs() {
        let mut vars = VariableSet::new();
        vars.get_or_new("IFS".into(), Scope::Global);
        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&vars);
        assert_eq!(field, dummy_field("foo bar baz"));
    }

    #[test]
    fn ifs_join_full_scalar_ifs() {
        let mut vars = VariableSet::new();
        vars.get_or_new("IFS".into(), Scope::Global)
            .assign("!?".into(), None)
            .unwrap();
        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&vars);
        assert_eq!(field, dummy_field("foo!bar!baz"));
    }

    #[test]
    fn ifs_join_full_array_ifs() {
        let mut vars = VariableSet::new();
        vars.get_or_new("IFS".into(), Scope::Global)
            .assign(Value::array(["-+", "abc"]), None)
            .unwrap();
        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&vars);
        assert_eq!(field, dummy_field("foo-bar-baz"));
    }

    #[test]
    fn ifs_join_full_empty_scalar_ifs() {
        let mut vars = VariableSet::new();
        vars.get_or_new("IFS".into(), Scope::Global)
            .assign("".into(), None)
            .unwrap();
        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&vars);
        assert_eq!(field, dummy_field("foobarbaz"));
    }

    #[test]
    fn ifs_join_full_empty_array_ifs() {
        let mut vars = VariableSet::new();
        vars.get_or_new("IFS".into(), Scope::Global)
            .assign(Value::Array(vec![]), None)
            .unwrap();
        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&vars);
        assert_eq!(field, dummy_field("foobarbaz"));

        vars.get_or_new("IFS".into(), Scope::Global)
            .assign(Value::array(["", "abc"]), None)
            .unwrap();
        let phrase = Full(vec![
            dummy_field("foo"),
            dummy_field("bar"),
            dummy_field("baz"),
        ]);
        let field = phrase.ifs_join(&vars);
        assert_eq!(field, dummy_field("foobarbaz"));
    }
}
