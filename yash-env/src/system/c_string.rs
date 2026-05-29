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
//! Public items in this module:
//!
//! - [`AsCStrArray`]: unsafe low-level contract for types that can expose
//!   a pointer to a null-terminated array of pointers to C-style strings.
//! - [`IntoCStrArray`]: ergonomic conversion trait used by [`Exec::execve`];
//!   accepts both native `AsCStrArray` implementors and convertible container
//!   types.
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
/// null-terminated byte strings. This trait abstracts over the details of how
/// these arrays are represented in Rust, allowing different types to be used as
/// long as they can provide the required pointer to the array of C-style
/// strings. The main requirement is that the array must be null-terminated and
/// that the pointers in the array must point to valid C-style strings.
/// Implementations of `Exec::execve` can then use this trait to accept
/// different types of string arrays and convert them into the required format
/// for the system call.
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
    // TODO: to_vec
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
/// themselves instead of borrowing them. The type parameter `T` represents the
/// collection of owned strings, which defaults to `Vec<CString>`. The
/// `OwnedCStrs` owns both the allocation for the array of pointers and the
/// collection of owned strings, ensuring that the pointers remain valid for the
/// lifetime of the `OwnedCStrs`.
///
/// The implementation of [`FromIterator`] allows you to create a `OwnedCStrs`
/// from an iterator of any `AsRef<CStr>` type, which can be useful when you
/// want to create an `AsCStrArray` from scratch and need to own the strings
/// themselves.
#[derive(Debug, Eq, PartialEq)]
pub struct OwnedCStrs<T = Vec<CString>> {
    pointers: Vec<*const c_char>,
    values: T,
}

impl<T> Sealed for OwnedCStrs<T> {}
unsafe impl<T> AsCStrArray for OwnedCStrs<T> {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.pointers.as_ptr()
    }
}

impl<T> OwnedCStrs<T> {
    /// Creates a new `OwnedCStrs<T>` from a collection of values that can be
    /// borrowed as `CStr`s.
    ///
    /// A reference to the given `values` must be convertible into an iterator
    /// of items that can be borrowed as `CStr`s, which are used to create the
    /// null-terminated array of pointers that will be returned by the
    /// [`as_ptr`](Self::as_ptr) method. The given `values` is stored in the
    /// `OwnedCStrs` for the lifetime of it to keep the strings valid and
    /// unmodified.
    ///
    /// Typically, `values` will be a `Vec<CString>`.
    #[must_use]
    pub fn new(values: T) -> Self
    where
        for<'a> &'a T: IntoIterator,
        for<'a> <&'a T as IntoIterator>::Item: AsRef<CStr>,
    {
        let pointers = null_terminated_pointers(&values).collect();

        // SAFETY: `null_terminated_pointers` creates a null-terminated array,
        // and the pointers in the array point to C-style strings borrowed from
        // the `values`. The strings remain valid and unmodified for the
        // lifetime of the `OwnedCStrs` because they are owned by it and not
        // mutated through the pointers.
        unsafe { Self::from_pointers_and_values(pointers, values) }
    }

    /// Creates a new `OwnedCStrs<T>` from its inner components.
    ///
    /// This function directly uses the given `pointers` as the content of the
    /// `OwnedCStrs`. The `pointers` is a null-terminated array of pointers to
    /// C-style strings that is returned by the [`as_ptr`](Self::as_ptr) method.
    /// The `values` is an object that is stored in the `OwnedCStrs` for the
    /// lifetime of it. Typically, `values` will be a `Vec<CString>`, and the
    /// `pointers` will point to the C-style strings contained in the
    /// `CString`s.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointers in the given `Vec` point to
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
    pub unsafe fn from_pointers_and_values(pointers: Vec<*const c_char>, values: T) -> Self {
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
    pub fn into_pointers_and_values(self) -> (Vec<*const c_char>, T) {
        (self.pointers, self.values)
    }
}

impl<T> Clone for OwnedCStrs<T>
where
    T: Clone,
    for<'a> &'a T: IntoIterator,
    for<'a> <&'a T as IntoIterator>::Item: AsRef<CStr>,
{
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

/// Converts a `Vec<T>` into a `OwnedCStrs<Vec<T>>`. (See [`OwnedCStrs::new`].)
impl<T> From<Vec<T>> for OwnedCStrs<Vec<T>>
where
    T: AsRef<CStr>,
{
    #[inline(always)]
    fn from(values: Vec<T>) -> Self {
        Self::new(values)
    }
}

/// Creates a `OwnedCStrs` from an iterator of values that can be borrowed as `CStr`s.
impl<A, T> FromIterator<A> for OwnedCStrs<T>
where
    A: AsRef<CStr>,
    T: FromIterator<A>,
{
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Self {
        let mut pointers = Vec::new();
        let values = iter
            .into_iter()
            .inspect(|s| pointers.push(s.as_ref().as_ptr()))
            .collect();
        pointers.push(std::ptr::null());
        unsafe { Self::from_pointers_and_values(pointers, values) }
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

impl<T> Sealed for Vec<T> where T: AsRef<CStr> {}
/// Converts a `Vec<T>` into a `OwnedCStrs<Vec<T>>`. (See [`OwnedCStrs::new`].)
impl<T> IntoCStrArray for Vec<T>
where
    T: AsRef<CStr>,
{
    type CStrArray = OwnedCStrs<Vec<T>>;

    #[inline(always)]
    fn into_c_str_array(self) -> OwnedCStrs<Vec<T>> {
        self.into()
    }
}
