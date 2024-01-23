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
use super::QuickExpand::{self, Interim, Ready};
use std::fmt::Debug;

#[derive(Debug)]
pub struct SliceExpandInterim<T: Debug> {
    phrase: Phrase,
    index: usize,
    item_interim: T,
}

/// Expands a slice of expandable items.
///
/// This implementation is typically used for expanding a word or text. Each
/// item in the slice is recursively expanded, and the results are merged into
/// one phrase by [`Phrase::append`].
///
/// If the slice has no item, the result is [one empty
/// field](Phrase::one_empty_field).
impl<T: Expand> Expand for [T] {
    type Interim = SliceExpandInterim<<T as Expand>::Interim>;

    fn quick_expand(&self, env: &mut Env<'_>) -> QuickExpand<Self::Interim> {
        if self.is_empty() {
            return Ready(Ok(Phrase::one_empty_field()));
        }

        let mut phrase = Phrase::zero_fields();
        for (index, item) in self.iter().enumerate() {
            match item.quick_expand(env) {
                Ready(Ok(item_phrase)) => phrase += item_phrase,
                Ready(Err(error)) => return Ready(Err(error)),
                Interim(item_interim) => {
                    return Interim(SliceExpandInterim {
                        phrase,
                        index,
                        item_interim,
                    })
                }
            }
        }
        Ready(Ok(phrase))
    }

    async fn async_expand(
        &self,
        env: &mut Env<'_>,
        interim: Self::Interim,
    ) -> Result<Phrase, Error> {
        let SliceExpandInterim {
            mut phrase,
            index,
            item_interim,
        } = interim;

        let mut iter = self[index..].iter();
        let item = iter
            .next()
            .expect("quick_expand and async_expand should be called on the same slice");
        phrase += item.async_expand(env, item_interim).await?;

        for item in iter {
            match item.quick_expand(env) {
                Ready(Ok(item_phrase)) => phrase += item_phrase,
                Ready(Err(error)) => return Err(error),
                Interim(item_interim) => phrase += item.async_expand(env, item_interim).await?,
            }
        }
        Ok(phrase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::attr::AttrChar;
    use crate::expansion::attr::Origin;
    use crate::expansion::Error;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::cell::Cell;

    fn dummy_attr_char(value: char) -> AttrChar {
        AttrChar {
            value,
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        }
    }

    enum Stub {
        Quick(Cell<Option<Result<Phrase, Error>>>),
        Async(Cell<Option<Result<Phrase, Error>>>),
    }

    impl Stub {
        fn new_quick(result: Result<Phrase, Error>) -> Self {
            Stub::Quick(Cell::new(Some(result)))
        }
        fn new_async(result: Result<Phrase, Error>) -> Self {
            Stub::Async(Cell::new(Some(result)))
        }
    }

    impl Debug for Stub {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Stub::Quick(_) => "Quick",
                Stub::Async(_) => "Async",
            }
            .fmt(f)
        }
    }

    impl Expand for Stub {
        type Interim = ();

        fn quick_expand(&self, _: &mut Env<'_>) -> QuickExpand<()> {
            if let Stub::Quick(cell) = self {
                Ready(
                    cell.take()
                        .expect("quick_expand should be called only once"),
                )
            } else {
                Interim(())
            }
        }

        async fn async_expand(&self, _: &mut Env<'_>, _: ()) -> Result<Phrase, Error> {
            assert_matches!(self, Stub::Async(cell) => {
                cell.take().expect("async_expand should be called only once")
            })
        }
    }

    #[test]
    fn empty_slice() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs: [Stub; 0] = [];
        assert_matches!(stubs.quick_expand(&mut env), Ready(result) => {
            assert_eq!(result, Ok(Phrase::one_empty_field()));
        });
    }

    #[test]
    fn slice_of_one_quick_item_returning_zero_fields() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [Stub::new_quick(Ok(Phrase::zero_fields()))];
        assert_matches!(stubs.quick_expand(&mut env), Ready(result) => {
            assert_eq!(result, Ok(Phrase::zero_fields()));
        });
    }

    #[test]
    fn slice_of_many_quick_items_returning_zero_fields() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [
            Stub::new_quick(Ok(Phrase::zero_fields())),
            Stub::new_quick(Ok(Phrase::zero_fields())),
            Stub::new_quick(Ok(Phrase::zero_fields())),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Ready(result) => {
            assert_eq!(result, Ok(Phrase::zero_fields()));
        });
    }

    #[test]
    fn slice_of_many_quick_items_each_returning_one_empty_field() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [
            Stub::new_quick(Ok(Phrase::one_empty_field())),
            Stub::new_quick(Ok(Phrase::one_empty_field())),
            Stub::new_quick(Ok(Phrase::one_empty_field())),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Ready(result) => {
            assert_eq!(result, Ok(Phrase::one_empty_field()));
        });
    }

    #[test]
    fn slice_of_many_quick_items_each_returning_one_non_empty_field() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let a = dummy_attr_char('a');
        let b = dummy_attr_char('b');
        let c = dummy_attr_char('c');
        let stubs = [
            Stub::new_quick(Ok(Phrase::Char(a))),
            Stub::new_quick(Ok(Phrase::Char(b))),
            Stub::new_quick(Ok(Phrase::Char(c))),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Ready(result) => {
            assert_eq!(result, Ok(Phrase::Field(vec![a, b, c])));
        });
    }

    #[test]
    fn slice_of_one_async_item_returning_zero_fields() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [Stub::new_async(Ok(Phrase::zero_fields()))];
        assert_matches!(stubs.quick_expand(&mut env), Interim(interim) => {
            let result = stubs
                .async_expand(&mut env, interim)
                .now_or_never()
                .unwrap();
            assert_eq!(result, Ok(Phrase::zero_fields()));
        });
    }

    #[test]
    fn slice_of_many_async_items_returning_zero_fields() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [
            Stub::new_async(Ok(Phrase::zero_fields())),
            Stub::new_async(Ok(Phrase::zero_fields())),
            Stub::new_async(Ok(Phrase::zero_fields())),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Interim(interim) => {
            let result = stubs
                .async_expand(&mut env, interim)
                .now_or_never()
                .unwrap();
            assert_eq!(result, Ok(Phrase::zero_fields()));
        });
    }

    #[test]
    fn slice_of_many_async_items_each_returning_one_empty_field() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let stubs = [
            Stub::new_async(Ok(Phrase::one_empty_field())),
            Stub::new_async(Ok(Phrase::one_empty_field())),
            Stub::new_async(Ok(Phrase::one_empty_field())),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Interim(interim) => {
            let result = stubs
                .async_expand(&mut env, interim)
                .now_or_never()
                .unwrap();
            assert_eq!(result, Ok(Phrase::one_empty_field()));
        });
    }

    #[test]
    fn slice_of_many_async_items_each_returning_one_non_empty_field() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let a = dummy_attr_char('a');
        let b = dummy_attr_char('b');
        let c = dummy_attr_char('c');
        let stubs = [
            Stub::new_async(Ok(Phrase::Char(a))),
            Stub::new_async(Ok(Phrase::Char(b))),
            Stub::new_async(Ok(Phrase::Char(c))),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Interim(interim) => {
            let result = stubs
                .async_expand(&mut env, interim)
                .now_or_never()
                .unwrap();
            assert_eq!(result, Ok(Phrase::Field(vec![a, b, c])));
        });
    }

    #[test]
    fn slice_of_quick_and_async_items() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let a = dummy_attr_char('a');
        let b = dummy_attr_char('b');
        let c = dummy_attr_char('c');
        let stubs = [
            Stub::new_quick(Ok(Phrase::Char(c))),
            Stub::new_async(Ok(Phrase::zero_fields())),
            Stub::new_quick(Ok(Phrase::Char(b))),
            Stub::new_async(Ok(Phrase::Char(a))),
            Stub::new_quick(Ok(Phrase::one_empty_field())),
        ];
        assert_matches!(stubs.quick_expand(&mut env), Interim(interim) => {
            let result = stubs
                .async_expand(&mut env, interim)
                .now_or_never()
                .unwrap();
            assert_eq!(result, Ok(Phrase::Field(vec![c, b, a])));
        });
    }
}
