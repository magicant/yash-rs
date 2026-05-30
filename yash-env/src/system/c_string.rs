// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Utilities for working with C-style strings ([`CStr`] and [`CString`]) in Rust
//!
//! This module provides abstractions for building and passing null-terminated
//! arrays of pointers to C-style strings, primarily for [`Exec::execve`].
//!
//! This module defines two main traits:
//!
//! - [`AsCStrArray`]: unsafe low-level contract for types that can expose
//!   a pointer to a null-terminated array of pointers to C-style strings.
//! - [`IntoCStrArray`]: ergonomic conversion trait used by [`Exec::execve`];
//!   accepts both native `AsCStrArray` implementors and convertible container
//!   types.
//!
//! And provides several struct implementations for different use cases:
//!
//! - [`CStrPtr`]: transparent wrapper for a raw pointer to an existing
//!   null-terminated C-string-pointer array.
//! - [`BorrowedCStrs`]: owning pointer-array wrapper over borrowed string data,
//!   suitable when you already have `&CStr`/`&CString` values.
//! - [`OwnedCStrs`]: owning wrapper that keeps both the strings and the pointer
//!   array alive together.
//!
//! In short, use [`BorrowedCStrs`] for borrowed inputs, [`OwnedCStrs`] for
//! owned inputs, and [`CStrPtr`] only when interoperating with raw FFI data.
//!
//! [`Exec::execve`]: super::Exec::execve

use std::ffi::{CStr, CString, c_char};
use std::marker::PhantomData;

/// Converts an iterator of `AsRef<CStr>` items into an iterator of pointers to
/// C-style strings, with a null pointer appended at the end.
fn null_terminated_pointers<I>(iter: I) -> impl Iterator<Item = *const c_char>
where
    I: IntoIterator,
    I::Item: AsRef<CStr>,
{
    iter.into_iter()
        .map(|s| s.as_ref().as_ptr())
        .chain(std::iter::once(std::ptr::null()))
}

/// Dummy trait to prevent external implementations of `AsCStrArray` and
/// `IntoCStrArray`
trait Sealed {}

/// Abstraction over an array of C-style strings, for arguments to [`Exec::execve`]
///
/// The native `execve` system call expects two arrays of C-style strings, which
/// must be passed as pointers to null-terminated arrays of pointers to
/// null-terminated byte strings. This trait abstracts over how ownership and
/// lifetime of these arrays and strings are managed in Rust, allowing
/// different types to be used as long as they can provide the required pointer
/// to the array of C-style strings. The main requirement is that the array must
/// be null-terminated and that the pointers in the array must point to valid
/// C-style strings. Implementations of `Exec::execve` can then use this trait
/// to accept different types of string arrays and to obtain the required
/// pointers for the system call without needing to know the details of how the
/// strings are stored or managed in Rust.
///
/// This trait is sealed to prevent external implementations, as the safety
/// guarantees of the methods depend on the implementor upholding certain
/// invariants about the pointers and the lifetime of the strings.
///
/// See also [`IntoCStrArray`] for types that can be converted into a
/// `AsCStrArray`.
///
/// [`Exec::execve`]: super::Exec::execve
#[allow(
    clippy::missing_safety_doc,
    reason = "users cannot implement sealed traits"
)]
#[expect(private_bounds, reason = "this trait is sealed")]
// SAFETY: This trait is unsafe because improper implementations can lead to
// undefined behavior when the pointers returned by `as_ptr` or `as_mut_ptr` are
// used. The implementation of the `execve` function assumes that the array and
// strings pointed to by these pointers are valid and remain unmodified while
// they are in use. Interior mutability may break these assumptions, so the
// implementor must ensure that no such mutations can occur.
pub unsafe trait AsCStrArray: Sealed {
    /// Returns a pointer to the array of C-style strings.
    ///
    /// The array must be null-terminated, i.e., the last pointer in the array
    /// must be a null pointer. Each pointer in the array must point to a valid
    /// C-style string (i.e., a null-terminated sequence of bytes). The array
    /// and strings must remain valid and unmodified until `self` is mutated or
    /// dropped. The caller must not mutate the array or strings through this
    /// pointer.
    fn as_ptr(&self) -> *const *const c_char;

    /// Returns a pointer to the array of C-style strings.
    ///
    /// This method just returns the same pointer as [`as_ptr`](Self::as_ptr),
    /// but with a different type signature. The mutability of the pointer is
    /// only for matching the expected type signature of [`execve`] and does not
    /// imply that the strings can actually be mutated through this pointer.
    /// The caller must not mutate the array or strings through this pointer.
    /// The array and strings must remain valid and unmodified until `self` is
    /// mutated or dropped.
    ///
    /// [`execve`]: super::Exec::execve
    #[inline(always)]
    fn as_mut_ptr(&mut self) -> *const *mut c_char {
        self.as_ptr().cast()
    }

    /// Constructs a `Vec<CString>` from the array of C-style strings.
    ///
    /// This method is a convenience for converting the array of C-style strings
    /// into a more Rust-friendly format. It iterates over the pointers in the
    /// array, converts each C-style string into a `CString`, and collects them
    /// into a `Vec<CString>`. The returned `Vec<CString>` owns the new
    /// allocations for the array and strings, so it is safe to use and modify
    /// independently of `self`.
    #[must_use]
    fn to_vec(&self) -> Vec<CString> {
        let mut vec = Vec::new();
        let mut ptr = self.as_ptr();
        loop {
            // SAFETY: The `as_ptr` method guarantees that the returned pointer
            // points to a null-terminated array of pointers to valid C-style
            // strings, so it's safe to read from the pointer.
            let c_str_ptr = unsafe { *ptr };
            if c_str_ptr.is_null() {
                break;
            }
            // SAFETY: Likewise, `c_str_ptr` points to a valid C-style string,
            // so it's safe to create a `CStr` from it.
            let c_str = unsafe { CStr::from_ptr(c_str_ptr) };
            vec.push(c_str.to_owned());
            // SAFETY: The `as_ptr` method guarantees that the array is
            // null-terminated, so it's safe to advance the pointer until we
            // reach the null pointer.
            ptr = unsafe { ptr.add(1) };
        }
        vec
    }
}

impl<T> Sealed for &T where T: Sealed + ?Sized {}
unsafe impl<T> AsCStrArray for &T
where
    T: AsCStrArray + ?Sized,
{
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        (self as &T).as_ptr()
    }
}

impl<T> Sealed for &mut T where T: Sealed + ?Sized {}
unsafe impl<T> AsCStrArray for &mut T
where
    T: AsCStrArray + ?Sized,
{
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        (self as &T).as_ptr()
    }
}

/// Simple wrapper to treat a raw pointer to an array of C-style strings as a [`AsCStrArray`]
///
/// This is useful for passing raw pointers obtained from other sources (e.g.,
/// FFI) to functions that expect an `AsCStrArray`. However, it is the caller's
/// responsibility to ensure that the pointer validly points to a
/// null-terminated array of pointers to valid C-style strings, and that the
/// array and strings remain valid for the required lifetime. This wrapper does
/// not take ownership of the array or strings.
///
/// You should prefer [`BorrowedCStrs`] or [`OwnedCStrs`] over this type when
/// possible, as they provide ownership and lifetime guarantees for the strings.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct CStrPtr(*const *const c_char);

impl CStrPtr {
    /// Create a new `CStrPtr` from a raw pointer to an array of C-style strings.
    ///
    /// The given pointer is directly returned by the [`as_ptr`](Self::as_ptr)
    /// method.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` points to a valid null-terminated
    /// array of pointers to valid C-style strings. The array and strings must
    /// remain valid and unmodified until the `CStrPtr` is dropped.
    #[must_use]
    pub unsafe fn new(ptr: *const *const c_char) -> Self {
        Self(ptr)
    }
}

impl Sealed for CStrPtr {}
unsafe impl AsCStrArray for CStrPtr {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.0
    }
}

/// An [`AsCStrArray`] backed by a [`Vec`] of borrowed [`CStr`]s
///
/// A `BorrowedCStrs<'a>` works as if it owns a `Vec<&'a CStr>`: it owns the
/// allocation for the array of pointers, but the `&CStr`s themselves are
/// borrowed from elsewhere. It is useful if you already have a collection of
/// `&CStr`s (or [`CString`]s) you can borrow and want to pass them to a
/// function that expects an `AsCStrArray`.
///
/// This struct implements [`FromIterator`], which can be used to create a
/// `BorrowedCStrs` from existing C-style strings. The implementation can also
/// be used via the [`IntoCStrArray`] trait.
#[derive(Clone, Debug)]
pub struct BorrowedCStrs<'a> {
    pointers: Vec<*const c_char>,
    phantom: PhantomData<Vec<&'a CStr>>,
}

impl<'a> Sealed for BorrowedCStrs<'a> {}
unsafe impl<'a> AsCStrArray for BorrowedCStrs<'a> {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.pointers.as_ptr()
    }
}

impl<'a, T> FromIterator<&'a T> for BorrowedCStrs<'a>
where
    T: AsRef<CStr> + ?Sized,
{
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = &'a T>>(iter: I) -> Self {
        let pointers = null_terminated_pointers(iter).collect();

        // SAFETY: The `null_terminated_pointers` function creates a
        // null-terminated array of pointers, and the pointers in the array
        // point to C-style strings borrowed from the input iterator. The
        // strings remain valid and unmodified for the lifetime of the
        // `BorrowedCStrs` because they are borrowed immutably through the
        // `PhantomData`.
        unsafe { Self::from_vec(pointers) }
    }
}

impl BorrowedCStrs<'_> {
    /// Creates a new `BorrowedCStrs` from a `Vec<*const c_char>`.
    ///
    /// This function directly uses the given pointers as the content of the
    /// `BorrowedCStrs`. The given pointers are returned by the
    /// [`as_ptr`](Self::as_ptr) method.
    ///
    /// This function takes ownership of the given `Vec`, but not the strings
    /// pointed to by the pointers.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointers in the given `Vec` point to
    /// valid C-style strings, and that the `Vec` is null-terminated (i.e., the
    /// last pointer is a null pointer). The array and strings must remain valid
    /// and unmodified for the required lifetime of the `BorrowedCStrs`.
    #[inline(always)]
    #[must_use]
    pub unsafe fn from_vec(pointers: Vec<*const c_char>) -> Self {
        Self {
            pointers,
            phantom: PhantomData,
        }
    }

    /// Consumes the `BorrowedCStrs` and returns the owned array of pointers as
    /// a `Vec<*const c_char>`.
    ///
    /// This function just returns the null-terminated array of pointers without
    /// any ownership or lifetime guarantees for the strings. The caller must
    /// ensure pointer validity if they intend to use the returned pointers.
    #[inline(always)]
    #[must_use]
    pub fn into_vec(self) -> Vec<*const c_char> {
        self.pointers
    }
}

/// Consumes a `BorrowedCStrs` and extracts the owned array of pointers as a
/// `Vec<*const c_char>`. (See [`BorrowedCStrs::into_vec`].)
impl From<BorrowedCStrs<'_>> for Vec<*const c_char> {
    #[inline(always)]
    fn from(c_str_vec: BorrowedCStrs) -> Self {
        c_str_vec.into_vec()
    }
}

/// A [`AsCStrArray`] backed by a collection of owned strings
///
/// This struct is a counterpart to [`BorrowedCStrs`] that owns the strings
/// themselves instead of borrowing them. `OwnedCStrs` owns both the allocation
/// for the array of pointers and the `Vec` of owned strings, ensuring that the
/// pointers remain valid for the lifetime of the `OwnedCStrs`.
///
/// The implementation of [`FromIterator`] allows you to create an `OwnedCStrs`
/// from an iterator of `CString`s, which can be useful when you want to create
/// an `AsCStrArray` from scratch and need to own the strings themselves.
#[derive(Debug)]
pub struct OwnedCStrs {
    pointers: Vec<*const c_char>,
    values: Vec<CString>,
}

impl Sealed for OwnedCStrs {}
unsafe impl AsCStrArray for OwnedCStrs {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.pointers.as_ptr()
    }
}

impl OwnedCStrs {
    /// Creates a new `OwnedCStrs` from a `Vec<CString>`.
    ///
    /// The given `values` is stored in the `OwnedCStrs` for the lifetime of it
    /// to keep the strings valid and unmodified. A null-terminated array of
    /// pointers to the C-style strings is created from the `values` and stored
    /// in the `OwnedCStrs` as well. The pointer to the array is returned by the
    /// [`as_ptr`](Self::as_ptr) method.
    #[must_use]
    pub fn new(values: Vec<CString>) -> Self {
        let pointers = null_terminated_pointers(&values).collect();

        // SAFETY: `null_terminated_pointers` creates a null-terminated array,
        // and the pointers in the array point to C-style strings borrowed from
        // the `values`. The strings remain valid and unmodified for the
        // lifetime of the `OwnedCStrs` because they are owned by it and not
        // mutated through the pointers.
        unsafe { Self::from_pointers_and_values(pointers, values) }
    }

    /// Creates a new `OwnedCStrs` from its inner components.
    ///
    /// This function directly uses the given `pointers` as the content of the
    /// `OwnedCStrs`. The `pointers` is a null-terminated array of pointers to
    /// C-style strings that is returned by the [`as_ptr`](Self::as_ptr) method.
    /// The `values` is a `Vec<CString>` stored in the `OwnedCStrs` for the
    /// lifetime of it.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointers in the first `Vec` point to
    /// valid C-style strings, and that the `Vec` is null-terminated (i.e., the
    /// last pointer is a null pointer). The strings must remain valid and
    /// unmodified for the lifetime of the `OwnedCStrs`.
    ///
    /// This function does not require that the `pointers` point to strings
    /// contained in `values`. However, if the strings pointed to by `pointers`
    /// are modified or dropped through other means while the `OwnedCStrs` is
    /// alive, it may lead to undefined behavior when the pointers are used. If
    /// the strings pointed to by `pointers` are not contained in `values`, you
    /// should consider using [`BorrowedCStrs`] instead, as it supports
    /// lifetime-based safety guarantees for the strings.
    #[inline(always)]
    #[must_use]
    pub unsafe fn from_pointers_and_values(
        pointers: Vec<*const c_char>,
        values: Vec<CString>,
    ) -> Self {
        Self { pointers, values }
    }

    /// Consumes the `OwnedCStrs` and returns the owned array of pointers and values.
    ///
    /// This function just returns its inner components. The caller is
    /// responsible for ensuring the validity of the returned pointers if they
    /// intend to use them. If `self` was created by [`OwnedCStrs::new`], the
    /// C-style strings pointed to by the returned pointers will be valid until
    /// the returned `values` are mutated or dropped.
    #[inline(always)]
    #[must_use]
    pub fn into_pointers_and_values(self) -> (Vec<*const c_char>, Vec<CString>) {
        (self.pointers, self.values)
    }
}

/// Clones an `OwnedCStrs` by cloning its owned strings and reconstructing the
/// pointer array to point to the new strings.
impl Clone for OwnedCStrs {
    fn clone(&self) -> Self {
        let strings = self.values.clone();
        // The new pointers must point to the new strings, so we create a new
        // `OwnedCStrs` from the cloned strings instead of just cloning the
        // pointers.
        Self::new(strings)
    }

    fn clone_from(&mut self, source: &Self) {
        self.values.clone_from(&source.values);
        // The new pointers must point to the new strings, so we need to
        // reinitialize the content of the pointers array.
        self.pointers.clear();
        self.pointers.extend(null_terminated_pointers(&self.values));
    }
}

/// Converts a `Vec<CString>` into an `OwnedCStrs`. (See [`OwnedCStrs::new`].)
impl From<Vec<CString>> for OwnedCStrs {
    #[inline(always)]
    fn from(values: Vec<CString>) -> Self {
        Self::new(values)
    }
}

/// Creates an `OwnedCStrs` from an iterator of values that can be converted into `CString`s.
impl<A> FromIterator<A> for OwnedCStrs
where
    A: Into<CString>,
{
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Self {
        let values = iter.into_iter().map(Into::into).collect();
        Self::new(values)
    }
}

/// Types that can be converted into an [`AsCStrArray`]
///
/// This trait is the bound that is actually required for the [`Exec::execve`]
/// function. By using this trait instead of [`AsCStrArray`] directly, `execve`
/// can accept types that either implement `AsCStrArray` directly or can be
/// converted into one, such as `Vec<CString>`. This reduces the need for users
/// to manually convert their string collections into `BorrowedCStrs`s or other
/// `AsCStrArray` implementors before passing them to `execve`.
///
/// Note that implementations of this trait may perform conversions that involve
/// heap allocations, so users should be aware of the potential performance
/// implications when using this trait with types that do not directly implement
/// `AsCStrArray`.
///
/// This trait is currently sealed to avoid breakage in a possible future
/// extension. Please open an
/// [issue](https://github.com/magicant/yash-rs/issues) if you have a use case
/// for implementing this trait for a type defined outside this crate and would
/// like it to be unsealed.
///
/// [`Exec::execve`]: super::Exec::execve
#[expect(private_bounds, reason = "this trait is sealed")]
pub trait IntoCStrArray: Sealed {
    /// The type that `self` can be converted into, which must implement `AsCStrArray`.
    type CStrArray: AsCStrArray;

    /// Converts `self` into a type that implements `AsCStrArray`.
    #[must_use]
    fn into_c_str_array(self) -> Self::CStrArray;
}

/// Any type that implements `AsCStrArray` can be converted into a
/// `AsCStrArray` by identity conversion.
impl<T: AsCStrArray> IntoCStrArray for T {
    type CStrArray = Self;

    #[inline(always)]
    fn into_c_str_array(self) -> Self::CStrArray {
        self
    }
}

impl<T> Sealed for &[T] where T: AsRef<CStr> {}
/// Converts a slice of `&CStr`s into a `BorrowedCStrs`. (See
/// [`BorrowedCStrs::from_iter`].)
impl<'a, T> IntoCStrArray for &'a [T]
where
    T: AsRef<CStr>,
{
    type CStrArray = BorrowedCStrs<'a>;

    #[inline(always)]
    fn into_c_str_array(self) -> BorrowedCStrs<'a> {
        BorrowedCStrs::from_iter(self)
    }
}

impl<T> Sealed for Vec<T> where T: Into<CString> {}
/// Converts a `Vec<T>` into an `OwnedCStrs`. (See [`OwnedCStrs::from_iter`].)
impl<T> IntoCStrArray for Vec<T>
where
    T: Into<CString>,
{
    type CStrArray = OwnedCStrs;

    #[inline(always)]
    fn into_c_str_array(self) -> OwnedCStrs {
        OwnedCStrs::from_iter(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_c_strings(words: &[&str]) -> Vec<CString> {
        words.iter().map(|s| CString::new(*s).unwrap()).collect()
    }

    #[test]
    fn as_c_str_array_to_vec() {
        let strings = make_c_strings(&["foo", "bar", "baz"]);
        let owned = OwnedCStrs::new(strings.clone());
        assert_eq!(owned.to_vec(), strings);

        // Empty array should round-trip to an empty Vec
        let empty = OwnedCStrs::new(vec![]);
        assert_eq!(empty.to_vec(), Vec::<CString>::new());
    }

    #[test]
    fn borrowed_c_strs_from_iterator() {
        let strings = make_c_strings(&["alpha", "beta"]);

        // Collect borrows into BorrowedCStrs
        let borrowed: BorrowedCStrs = strings.iter().collect();

        // Content round-trips correctly
        assert_eq!(borrowed.to_vec(), strings);

        // The pointers in the array must actually borrow from `strings`
        let ptr = borrowed.as_ptr();
        unsafe {
            assert_eq!(*ptr, strings[0].as_ptr());
            assert_eq!(*ptr.add(1), strings[1].as_ptr());
            // Array is null-terminated
            assert!((*ptr.add(2)).is_null());
        }
    }

    #[test]
    fn owned_c_strs_new() {
        let strings = make_c_strings(&["hello", "world"]);

        let owned = OwnedCStrs::new(strings.clone());

        // Content is preserved
        assert_eq!(owned.to_vec(), strings);

        // Pointers must be self-consistent (point into the owned values)
        let (pointers, values) = owned.into_pointers_and_values();
        for (i, value) in values.iter().enumerate() {
            assert_eq!(pointers[i], value.as_ptr());
        }
        assert!(pointers[values.len()].is_null());
    }

    #[test]
    fn owned_c_strs_clone() {
        let strings = make_c_strings(&["foo", "bar"]);
        let owned = OwnedCStrs::new(strings.clone());

        let cloned = owned.clone();

        // Cloned content matches the original
        assert_eq!(cloned.to_vec(), strings);

        // The clone must own its own copy of the strings, so the pointers
        // must point to different memory than the original's pointers.
        let orig_ptr = owned.as_ptr();
        let clone_ptr = cloned.as_ptr();
        unsafe {
            assert_ne!(orig_ptr, clone_ptr);
            assert_ne!(*orig_ptr, *clone_ptr);
            assert_ne!(*orig_ptr.add(1), *clone_ptr.add(1));
        }
    }

    #[test]
    fn owned_c_strs_clone_from() {
        let strings1 = make_c_strings(&["one"]);
        let strings2 = make_c_strings(&["two", "three"]);
        let source = OwnedCStrs::new(strings2.clone());
        let mut dest = OwnedCStrs::new(strings1);

        dest.clone_from(&source);

        assert_eq!(dest.to_vec(), strings2);

        // dest's pointers must be self-consistent (point into dest's own values)
        let (pointers, values) = dest.into_pointers_and_values();
        for (i, value) in values.iter().enumerate() {
            assert_eq!(pointers[i], value.as_ptr());
        }
        assert!(pointers[values.len()].is_null());
    }

    #[test]
    fn owned_c_strs_from_iterator() {
        let strings = make_c_strings(&["x", "y", "z"]);

        // Collect owned CStrings via the FromIterator impl
        let owned: OwnedCStrs = strings.iter().cloned().collect();

        assert_eq!(owned.to_vec(), strings);

        // The pointers in the array must be self-consistent (point into the owned values)
        let (pointers, values) = owned.into_pointers_and_values();
        for (i, value) in values.iter().enumerate() {
            assert_eq!(pointers[i], value.as_ptr());
        }
        assert!(pointers[values.len()].is_null());
    }
}
