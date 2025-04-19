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

//! Future-related utilities

use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future that either returns a precomputed value or delegates to a
/// heap-allocated, type-erased future.
///
/// `FlexFuture` works as a selective wrapper for [`std::future::Ready`],
/// [`std::future::Pending`], and a generic future in a [`Box`]. When a function
/// needs to return a future, it can use `FlexFuture` to possibly avoid heap
/// allocation if the future is already known to be ready.
///
/// This type does not have a lifetime parameter, so the contained future must
/// have a `'static` lifetime. This is because `FlexFuture` is also used in
/// [`SharedSystem`](super::SharedSystem), which performs dynamic lifetime
/// checking to access its internal state guarded by a `RefCell`. Instead of
/// borrowing the system, the future must share ownership of the system to keep
/// it alive until the future is resolved.
pub enum FlexFuture<T> {
    /// Future that is already ready with a value
    Ready(std::future::Ready<T>),
    /// Future that is pending and will never resolve
    Pending(std::future::Pending<T>),
    /// Heap-allocated, type-erased future
    Generic(Pin<Box<dyn Future<Output = T>>>),
}

impl<T: Debug> Debug for FlexFuture<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlexFuture::Ready(ready) => ready.fmt(f),
            FlexFuture::Pending(pending) => pending.fmt(f),
            FlexFuture::Generic(_) => f.debug_tuple("Generic").finish_non_exhaustive(),
        }
    }
}

impl<T> From<T> for FlexFuture<T> {
    fn from(value: T) -> Self {
        FlexFuture::Ready(std::future::ready(value))
    }
}

impl<T> From<std::future::Ready<T>> for FlexFuture<T> {
    fn from(ready: std::future::Ready<T>) -> Self {
        FlexFuture::Ready(ready)
    }
}

impl<T> From<std::future::Pending<T>> for FlexFuture<T> {
    fn from(pending: std::future::Pending<T>) -> Self {
        FlexFuture::Pending(pending)
    }
}

impl<T> From<Pin<Box<dyn Future<Output = T>>>> for FlexFuture<T> {
    fn from(future: Pin<Box<dyn Future<Output = T>>>) -> Self {
        FlexFuture::Generic(future)
    }
}

impl<T> From<Box<dyn Future<Output = T>>> for FlexFuture<T> {
    fn from(future: Box<dyn Future<Output = T>>) -> Self {
        FlexFuture::Generic(Box::into_pin(future))
    }
}

impl<T> FlexFuture<T> {
    /// Creates a new `FlexFuture` from any future.
    ///
    /// This function allocates memory for the future. If the future is already
    /// allocated on the heap, use [`FlexFuture::from`] instead.
    pub fn boxed<F>(f: F) -> Self
    where
        F: Future<Output = T> + 'static,
    {
        FlexFuture::Generic(Box::pin(f))
    }

    /// Converts this `FlexFuture` into a `Pin<Box<dyn Future<Output = T>>`.
    pub fn into_boxed(self) -> Pin<Box<dyn Future<Output = T>>>
    where
        T: 'static,
    {
        match self {
            FlexFuture::Ready(ready) => Box::pin(ready),
            FlexFuture::Pending(pending) => Box::pin(pending),
            FlexFuture::Generic(generic) => generic,
        }
    }
}

impl<T: 'static> From<FlexFuture<T>> for Pin<Box<dyn Future<Output = T>>> {
    fn from(future: FlexFuture<T>) -> Self {
        future.into_boxed()
    }
}

impl<T> Future for FlexFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match self.get_mut() {
            FlexFuture::Ready(ready) => Pin::new(ready).poll(cx),
            FlexFuture::Pending(pending) => Pin::new(pending).poll(cx),
            FlexFuture::Generic(generic) => generic.as_mut().poll(cx),
        }
    }
}
