// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Types for storing arbitrary data in the environment
//!
//! This module provides [`DataSet`] for storing arbitrary data in [`Env`].
//! It internally uses [`Any`] to store data of arbitrary types.
//!
//! [`Env`]: crate::Env

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::Debug;

/// Entry in the [`DataSet`]
#[derive(Debug)]
struct Entry {
    data: Box<dyn Any>,
    clone: fn(&dyn Any) -> Box<dyn Any>,
}
// TODO: When dyn upcasting coercion[1] is stabilized, we will be able to define
// `trait Data: Any + Clone {}` and insert `Box<dyn Data>` into `DataSet`
// without the need to store the `clone` function.
// [1]: https://github.com/rust-lang/rust/issues/65991

impl Clone for Entry {
    fn clone(&self) -> Self {
        Self {
            data: (self.clone)(&*self.data),
            clone: self.clone,
        }
    }
}

/// Clones data of the specified type
///
/// This function is used to clone data stored in the [`DataSet`].
#[must_use]
fn clone<T: Clone + 'static>(data: &dyn Any) -> Box<dyn Any> {
    Box::new(data.downcast_ref::<T>().unwrap().clone())
}

/// Collection of arbitrary data
///
/// This struct is used to store arbitrary data in the environment.
/// Data stored in this struct are identified by their [`TypeId`], so you cannot
/// store multiple data instances of the same type.
#[derive(Clone, Debug, Default)]
pub struct DataSet {
    inner: HashMap<TypeId, Entry>,
}

impl DataSet {
    /// Creates a new empty `DataSet`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new data into the `DataSet`.
    ///
    /// If data of the same type is already stored in `self`, it is replaced.
    pub fn insert<T: Clone + 'static>(&mut self, data: Box<T>) -> Option<Box<T>> {
        let clone = clone::<T>;
        let entry = Entry { data, clone };
        self.inner
            .insert(TypeId::of::<T>(), entry)
            .map(|old| old.data.downcast().unwrap())
    }

    /// Obtains a reference to the data of the specified type.
    #[must_use]
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.inner
            .get(&TypeId::of::<T>())
            .map(|entry| entry.data.downcast_ref().unwrap())
    }

    /// Obtains a mutable reference to the data of the specified type.
    #[must_use]
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.inner
            .get_mut(&TypeId::of::<T>())
            .map(|entry| entry.data.downcast_mut().unwrap())
    }

    /// Obtains a reference to the data of the specified type, or inserts a new
    /// data if it does not exist.
    ///
    /// If the data does not exist, the data is created by calling the provided
    /// closure and inserted into the `DataSet`, and a reference to the data is
    /// returned. If the data already exists, a reference to the existing data
    /// is returned and the closure is not called.
    pub fn get_or_insert_with<T, F>(&mut self, f: F) -> &mut T
    where
        T: Clone + 'static,
        F: FnOnce() -> Box<T>,
    {
        self.inner
            .entry(TypeId::of::<T>())
            .or_insert_with(|| {
                let data = f();
                let clone = clone::<T>;
                Entry { data, clone }
            })
            .data
            .downcast_mut()
            .unwrap()
    }

    /// Removes the data of the specified type from the `DataSet`.
    ///
    /// Returns the data if it exists.
    pub fn remove<T: 'static>(&mut self) -> Option<Box<T>> {
        self.inner
            .remove(&TypeId::of::<T>())
            .map(|entry| entry.data.downcast().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut data_set = DataSet::new();
        let old = data_set.insert(Box::new(42i32));
        assert_eq!(old, None);
        assert_eq!(data_set.get::<i32>(), Some(&42));
    }

    #[test]
    fn insert_again() {
        let mut data_set = DataSet::new();
        data_set.insert(Box::new(42i32));
        let old = data_set.insert(Box::new(43i32));
        assert_eq!(old, Some(Box::new(42)));
        assert_eq!(data_set.get::<i32>(), Some(&43));
    }

    #[test]
    fn get_mut() {
        let mut data_set = DataSet::new();
        data_set.insert(Box::new(42i32));
        let data = data_set.get_mut::<i32>().unwrap();
        assert_eq!(data, &42);
        *data = 43;
        assert_eq!(data_set.get::<i32>(), Some(&43));
    }

    #[test]
    fn get_or_insert_with() {
        let mut data_set = DataSet::new();
        let data = data_set.get_or_insert_with(|| Box::new(0i8));
        assert_eq!(data, &0);
        *data = 1;
        let data = data_set.get_or_insert_with::<i8, _>(|| unreachable!());
        assert_eq!(data, &1);
    }

    #[test]
    fn remove_existing() {
        let mut data_set = DataSet::new();
        data_set.insert(Box::new(42i32));
        let data = data_set.remove::<i32>().unwrap();
        assert_eq!(*data, 42);
    }

    #[test]
    fn remove_nonexisting() {
        let mut data_set = DataSet::new();
        let data = data_set.remove::<i32>();
        assert_eq!(data, None);
    }

    #[test]
    fn clone() {
        let mut data_set = DataSet::new();
        data_set.insert::<i32>(Box::new(42));
        let clone = data_set.clone();
        assert_eq!(clone.get::<i32>(), Some(&42));
    }
}
