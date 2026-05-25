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

use std::ffi::{CStr, CString, c_char};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Creates an iterator that yields a single null pointer, for terminating
/// arrays of C-style strings.
#[inline]
fn one_null() -> impl Iterator<Item = *const c_char> {
    std::iter::once(std::ptr::null())
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
/// You should prefer [`CStrVec`] or [`CStringVec`] over this type when
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

/// An [`AsCStrArray`] backed by a [`Vec`] of [`&CStr`](CStr)s
///
/// A `CStrVec` works as if it owns a `Vec<&'a CStr>`: it owns the allocation
/// for the array of pointers, but the `&CStr`s themselves are borrowed from
/// elsewhere. It is useful if you already have a collection of `&CStr`s (or
/// `CString`s) you can borrow and want to pass them to a function that expects
/// an `AsCStrArray`.
///
/// This struct implements `FromIterator`, which can be used to create a
/// `CStrVec` from existing C-style strings. The implementation can also be used
/// via the [`IntoCStrArray`] trait.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CStrVec<'a> {
    pointers: Vec<*const c_char>,
    phantom: PhantomData<Vec<&'a CStr>>,
}

impl<'a> Sealed for CStrVec<'a> {}
unsafe impl<'a> AsCStrArray for CStrVec<'a> {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.pointers.as_ptr()
    }
}

// TODO: Consider merging these two `FromIterator` impls into a single one that accepts an iterator of `impl AsRef<CStr>`, which would allow both `&CStr` and `CString
/// Creates a `CStrVec` from an iterator of `&CStr`s
impl<'a> FromIterator<&'a CStr> for CStrVec<'a> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = &'a CStr>>(iter: T) -> Self {
        fn inner<'b, I: Iterator<Item = &'b CStr>>(iter: I) -> CStrVec<'b> {
            CStrVec {
                pointers: iter.map(CStr::as_ptr).chain(one_null()).collect(),
                phantom: PhantomData,
            }
        }
        inner(iter.into_iter())
    }
}

/// Creates a `CStrVec` from an iterator of `&CString`s
impl<'a> FromIterator<&'a CString> for CStrVec<'a> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = &'a CString>>(iter: T) -> Self {
        fn inner<'b, I: Iterator<Item = &'b CString>>(iter: I) -> CStrVec<'b> {
            iter.map(CString::as_c_str).collect()
        }
        inner(iter.into_iter())
    }
}

impl CStrVec<'_> {
    /// Consumes the `CStrVec` and returns the owned array of pointers as a
    /// `Vec<*const c_char>`.
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

/// Consumes a `CStrVec` and extracts the owned array of pointers as a
/// `Vec<*const c_char>`. (See [`CStrVec::into_vec`].)
impl From<CStrVec<'_>> for Vec<*const c_char> {
    #[inline(always)]
    fn from(c_str_vec: CStrVec) -> Self {
        c_str_vec.into_vec()
    }
}

/// A [`AsCStrArray`] backed by a [`Vec`] of [`CString`]s
///
/// A `CStringVec` owns a `Vec<CString>` and the allocation for the array of
/// pointers. It is useful if you want to create an `AsCStrArray` from scratch
/// and need to own the `CString`s themselves.
#[derive(Debug)]
pub struct CStringVec {
    pointers: Vec<*const c_char>,
    strings: Vec<CString>,
}

impl Sealed for CStringVec {}
unsafe impl AsCStrArray for CStringVec {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.pointers.as_ptr()
    }
}

impl CStringVec {
    /// Creates a new `CStringVec` from a vector of `CString`s.
    ///
    /// The given `CString`s are stored in the `CStringVec` to ensure that they
    /// remain valid for the required lifetime. The `CStringVec` also allocates
    /// a null-terminated array of pointers to the `CString`s, which is returned
    /// by the [`as_ptr`](Self::as_ptr) method.
    #[must_use]
    pub fn new(strings: Vec<CString>) -> Self {
        let pointers = strings
            .iter()
            .map(|s| s.as_ptr())
            .chain(one_null())
            .collect();
        Self { pointers, strings }
    }

    /// Consumes the `CStringVec` and returns the owned `CString`s.
    #[must_use]
    pub fn into_c_strings(self) -> Vec<CString> {
        self.strings
    }
}

impl Clone for CStringVec {
    fn clone(&self) -> Self {
        let strings = self.strings.clone();
        // The new pointers must point to the new strings, so we create a new
        // `CStringVec` from the cloned strings instead of just cloning the
        // pointers.
        Self::new(strings)
    }

    fn clone_from(&mut self, source: &Self) {
        self.strings.clone_from(&source.strings);
        // The new pointers must point to the new strings, so we need to
        // reinitialize the content of the pointers array.
        self.pointers.clear();
        self.pointers
            .extend(self.strings.iter().map(|s| s.as_ptr()).chain(one_null()));
    }
}

/// Compares the contained `CString`s for equality.
impl PartialEq for CStringVec {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.strings == other.strings
    }
}

impl Eq for CStringVec {}

/// Hashes the contained `CString`s.
impl Hash for CStringVec {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.strings.hash(state);
    }
}

/// Compares the contained `CString`s for ordering.
impl PartialOrd for CStringVec {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares the contained `CString`s for ordering.
impl Ord for CStringVec {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.strings.cmp(&other.strings)
    }
}

/// Converts a `Vec<CString>` into a `CStringVec`. (See [`CStringVec::new`].)
impl From<Vec<CString>> for CStringVec {
    #[inline(always)]
    fn from(strings: Vec<CString>) -> Self {
        Self::new(strings)
    }
}

/// Consumes a `CStringVec` and extracts the owned `CString`s as a `Vec<CString>`
impl From<CStringVec> for Vec<CString> {
    #[inline(always)]
    fn from(c_string_vec: CStringVec) -> Self {
        c_string_vec.into_c_strings()
    }
}

/// Creates a `CStringVec` from an iterator of `CString`s
impl<T> FromIterator<T> for CStringVec
where
    T: Into<CString>,
{
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        fn inner<T: Into<CString>, I: Iterator<Item = T>>(iter: I) -> CStringVec {
            CStringVec::new(iter.map(Into::into).collect())
        }
        inner(iter.into_iter())
    }
}

/// Types that can be converted into an [`AsCStrArray`]
///
/// This trait is the bound that is actually required for the [`Exec::execve`]
/// function. By using this trait instead of [`AsCStrArray`] directly, `execve`
/// can accept types that either implement `AsCStrArray` directly or can be
/// converted into one, such as `Vec<CString>`. This reduces the need for users
/// to manually convert their string collections into `CStrVec`s or other
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
    type CStrArray: AsCStrArray;

    fn into_c_str_array(self) -> Self::CStrArray;
}

/// Any type that implements `AsCStrArray` can be converted into a
/// `AsCStrArray` by identity conversion.
impl<T: AsCStrArray> IntoCStrArray for T {
    type CStrArray = Self;

    fn into_c_str_array(self) -> Self::CStrArray {
        self
    }
}

impl<'a> Sealed for &'a [&'a CStr] {}
// TODO: Consider merging these two `IntoCStrArray` impls into a single one that accepts a slice of `impl AsRef<CStr>`, which would allow both `&CStr` and `CString`
/// Converts a slice of `&CStr`s into a `CStrVec`
impl<'a> IntoCStrArray for &'a [&'a CStr] {
    type CStrArray = CStrVec<'a>;

    fn into_c_str_array(self) -> CStrVec<'a> {
        self.iter().cloned().collect()
    }
}

impl Sealed for &[CString] {}
/// Converts a slice of `&CString`s into a `CStrVec`
impl<'a> IntoCStrArray for &'a [CString] {
    type CStrArray = CStrVec<'a>;

    fn into_c_str_array(self) -> CStrVec<'a> {
        self.iter().map(CString::as_c_str).collect()
    }
}

impl Sealed for Vec<CString> {}
/// Converts a `Vec<CString>` into a `CStringVec`. (See [`CStringVec::new`].)
impl IntoCStrArray for Vec<CString> {
    type CStrArray = CStringVec;

    #[inline(always)]
    fn into_c_str_array(self) -> CStringVec {
        self.into()
    }
}
