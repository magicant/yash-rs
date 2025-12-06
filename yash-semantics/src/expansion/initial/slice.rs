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

//! Implementation of [`Expand`] for slices.

use super::super::Error;
use super::Env;
use super::Expand;
use super::Phrase;

/// Expands a slice of expandable items.
///
/// This implementation is typically used for expanding a word or text. Each
/// item in the slice is recursively expanded, and the results are merged into
/// one phrase by [`Phrase::append`].
///
/// If the slice has no item, the result is [one empty
/// field](Phrase::one_empty_field).
impl<S, T: Expand<S>> Expand<S> for [T] {
    async fn expand(&self, env: &mut Env<'_, S>) -> Result<Phrase, Error> {
        if self.is_empty() {
            return Ok(Phrase::one_empty_field());
        }

        let mut phrase = Phrase::zero_fields();
        for item in self {
            phrase += item.expand(env).await?;
        }
        Ok(phrase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::Error;
    use crate::expansion::attr::AttrChar;
    use crate::expansion::attr::Origin;
    use futures_util::FutureExt as _;
    use std::cell::Cell;

    fn dummy_attr_char(value: char) -> AttrChar {
        AttrChar {
            value,
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        }
    }

    struct Stub(Cell<Option<Result<Phrase, Error>>>);

    impl Stub {
        fn new(result: Result<Phrase, Error>) -> Self {
            Stub(Cell::new(Some(result)))
        }
    }

    impl Expand for Stub {
        async fn expand(&self, _: &mut Env<'_>) -> Result<Phrase, Error> {
            self.0.take().expect("expand should be called only once")
        }
    }

    #[test]
    fn empty_slice() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs: [Stub; 0] = [];
        let result = stubs.expand(&mut env).now_or_never().unwrap();
        assert_eq!(result, Ok(Phrase::one_empty_field()));
    }

    #[test]
    fn slice_of_one_item_returning_zero_fields() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [Stub::new(Ok(Phrase::zero_fields()))];
        let result = stubs.expand(&mut env).now_or_never().unwrap();
        assert_eq!(result, Ok(Phrase::zero_fields()));
    }

    #[test]
    fn slice_of_many_items_returning_zero_fields() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [
            Stub::new(Ok(Phrase::zero_fields())),
            Stub::new(Ok(Phrase::zero_fields())),
            Stub::new(Ok(Phrase::zero_fields())),
        ];
        let result = stubs.expand(&mut env).now_or_never().unwrap();
        assert_eq!(result, Ok(Phrase::zero_fields()));
    }

    #[test]
    fn slice_of_many_items_each_returning_one_empty_field() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [
            Stub::new(Ok(Phrase::one_empty_field())),
            Stub::new(Ok(Phrase::one_empty_field())),
            Stub::new(Ok(Phrase::one_empty_field())),
        ];
        let result = stubs.expand(&mut env).now_or_never().unwrap();
        assert_eq!(result, Ok(Phrase::one_empty_field()));
    }

    #[test]
    fn slice_of_many_items_each_returning_one_non_empty_field() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let a = dummy_attr_char('a');
        let b = dummy_attr_char('b');
        let c = dummy_attr_char('c');
        let stubs = [
            Stub::new(Ok(Phrase::Char(a))),
            Stub::new(Ok(Phrase::Char(b))),
            Stub::new(Ok(Phrase::Char(c))),
        ];
        let result = stubs.expand(&mut env).now_or_never().unwrap();
        assert_eq!(result, Ok(Phrase::Field(vec![a, b, c])));
    }
}
