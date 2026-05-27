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

#[cfg(doc)]
use std::ffi::CString;
use std::ffi::{CStr, c_char};
use std::hash::{Hash, Hasher};
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
/// You should prefer [`CStrVec`] or [`PtrVec`] over this type when possible, as
/// they provide ownership and lifetime guarantees for the strings.
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
/// A `CStrVec<'a>` works as if it owns a `Vec<&'a CStr>`: it owns the
/// allocation for the array of pointers, but the `&CStr`s themselves are
/// borrowed from elsewhere. It is useful if you already have a collection of
/// `&CStr`s (or [`CString`]s) you can borrow and want to pass them to a
/// function that expects an `AsCStrArray`.
///
/// This struct implements [`FromIterator`], which can be used to create a
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

impl<'a, T> FromIterator<&'a T> for CStrVec<'a>
where
    T: AsRef<CStr> + ?Sized,
{
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = &'a T>>(iter: I) -> Self {
        let pointers = null_terminated_pointers(iter).collect();

        // SAFETY: The `null_terminated_pointers` function creates a
        // null-terminated array of pointers, and the pointers in the array
        // point to C-style strings borrowed from the input iterator. The
        // strings remain valid and unmodified for the lifetime of the `CStrVec`
        // because they are borrowed immutably through the `PhantomData`.
        unsafe { Self::from_vec(pointers) }
    }
}

impl CStrVec<'_> {
    /// Creates a new `CStrVec` from a `Vec<*const c_char>`.
    ///
    /// This function directly uses the given pointers as the content of the
    /// `CStrVec`. The given pointers are returned by the
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
    /// and unmodified for the required lifetime of the `CStrVec`.
    #[inline(always)]
    #[must_use]
    pub unsafe fn from_vec(pointers: Vec<*const c_char>) -> Self {
        Self {
            pointers,
            phantom: PhantomData,
        }
    }

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

/// A [`AsCStrArray`] backed by a [`Vec`] of owned C-style strings
///
/// A `PtrVec<T>` owns a `Vec<T>` and the allocation for the array of pointers.
/// In the most typical case, `T` will be [`CString`], but it can be any type
/// that implements `AsRef<CStr>`. `PtrVec` is useful when you want to create an
/// `AsCStrArray` from scratch and need to own the strings themselves. The
/// implementation of `FromIterator` allows you to create a `PtrVec` from an
/// iterator of any `AsRef<CStr>` type.
#[derive(Debug)]
pub struct PtrVec<T> {
    pointers: Vec<*const c_char>,
    values: Vec<T>,
}

impl<T> Sealed for PtrVec<T> {}
unsafe impl<T> AsCStrArray for PtrVec<T> {
    #[inline(always)]
    fn as_ptr(&self) -> *const *const c_char {
        self.pointers.as_ptr()
    }
}

impl<T> PtrVec<T> {
    /// Creates a new `PtrVec<T>` from a `Vec<T>`.
    ///
    /// The given values are stored in the `PtrVec` to ensure that they remain
    /// valid for the required lifetime. The `PtrVec` also allocates a
    /// null-terminated array of pointers to the C-style strings, which is
    /// returned by the [`as_ptr`](Self::as_ptr) method.
    #[must_use]
    pub fn new(values: Vec<T>) -> Self
    where
        T: AsRef<CStr>,
    {
        let pointers = null_terminated_pointers(&values).collect();

        // SAFETY: `null_terminated_pointers` creates a null-terminated array,
        // and the pointers in the array point to C-style strings borrowed from
        // the `values`. The strings remain valid and unmodified for the
        // lifetime of the `PtrVec` because they are owned by it and not mutated
        // through the pointers.
        unsafe { Self::from_pointers_and_values(pointers, values) }
    }

    /// Creates a new `PtrVec<T>` from its inner components.
    ///
    /// This function directly uses the given pointers as the content of the
    /// `PtrVec`. The `pointers` is a null-terminated array of pointers to
    /// C-style strings that is returned by the [`as_ptr`](Self::as_ptr) method.
    /// The `values` is an array of objects that is stored in the `PtrVec` for
    /// the lifetime of it. Typically, `values` will be a `Vec<CString>`, and
    /// the `pointers` will point to the C-style strings contained in the
    /// `CString`s.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the pointers in the given `Vec` point to
    /// valid C-style strings, and that the `Vec` is null-terminated (i.e., the
    /// last pointer is a null pointer). The strings must remain valid and
    /// unmodified for the lifetime of the `PtrVec`.
    #[must_use]
    pub unsafe fn from_pointers_and_values(pointers: Vec<*const c_char>, values: Vec<T>) -> Self {
        Self { pointers, values }
    }

    /// Consumes the `PtrVec` and returns the owned array of pointers and values.
    ///
    /// This function just returns its inner components. The caller is
    /// responsible for ensuring the validity of the returned pointers if they
    /// intend to use them. The C-style strings pointed to by the returned
    /// pointers will be valid until the returned `values` are mutated or
    /// dropped.
    #[inline(always)]
    #[must_use]
    pub fn into_pointers_and_values(self) -> (Vec<*const c_char>, Vec<T>) {
        (self.pointers, self.values)
    }
}

impl<T> Clone for PtrVec<T>
where
    T: Clone + AsRef<CStr>,
{
    fn clone(&self) -> Self {
        let strings = self.values.clone();
        // The new pointers must point to the new strings, so we create a new
        // `PtrVec` from the cloned strings instead of just cloning the
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

/// Compares the contained values for equality.
impl<T> PartialEq for PtrVec<T>
where
    T: PartialEq,
{
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

/// Compares the contained values for equality.
impl<T> Eq for PtrVec<T> where T: Eq {}

/// Hashes the contained values.
impl<T> Hash for PtrVec<T>
where
    T: Hash,
{
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.values.hash(state);
    }
}

/// Compares the contained values for ordering.
impl<T> PartialOrd for PtrVec<T>
where
    T: PartialOrd,
{
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.values.partial_cmp(&other.values)
    }
}

/// Compares the contained values for ordering.
impl<T> Ord for PtrVec<T>
where
    T: Ord,
{
    #[inline(always)]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.values.cmp(&other.values)
    }
}

/// Converts a `Vec<T>` into a `PtrVec<T>`. (See [`PtrVec::new`].)
impl<T> From<Vec<T>> for PtrVec<T>
where
    T: AsRef<CStr>,
{
    #[inline(always)]
    fn from(values: Vec<T>) -> Self {
        Self::new(values)
    }
}

/// Creates a `PtrVec` from an iterator of values that can be borrowed as `CStr`s.
impl<T> FromIterator<T> for PtrVec<T>
where
    T: AsRef<CStr>,
{
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::new(Vec::from_iter(iter))
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
/// Converts a slice of `&CStr`s into a `CStrVec`. (See [`CStrVec::from_iter`].)
impl<'a, T> IntoCStrArray for &'a [T]
where
    T: AsRef<CStr>,
{
    type CStrArray = CStrVec<'a>;

    #[inline(always)]
    fn into_c_str_array(self) -> CStrVec<'a> {
        CStrVec::from_iter(self)
    }
}

impl<T> Sealed for Vec<T> where T: AsRef<CStr> {}
/// Converts a `Vec<T>` into a `PtrVec<T>`. (See [`PtrVec::new`].)
impl<T> IntoCStrArray for Vec<T>
where
    T: AsRef<CStr>,
{
    type CStrArray = PtrVec<T>;

    #[inline(always)]
    fn into_c_str_array(self) -> PtrVec<T> {
        self.into()
    }
}
