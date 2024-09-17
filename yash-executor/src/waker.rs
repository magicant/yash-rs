// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Implementation of `Waker`
//!
//! This module provides a function to convert a `Task` into a `Waker`. The
//! `RawWaker`'s data pointer is a `Rc<Task>`, and the `RawWakerVTable` contains
//! functions to clone, wake, wake by reference, and drop the `Rc<Task>`.

use crate::Task;
use alloc::rc::Rc;
use core::task::{RawWaker, RawWakerVTable, Waker};

unsafe fn clone(data: *const ()) -> RawWaker {
    Rc::<Task>::increment_strong_count(data.cast());
    RawWaker::new(data, VTABLE)
}

unsafe fn wake(data: *const ()) {
    Rc::<Task>::from_raw(data.cast()).wake();
}

unsafe fn wake_by_ref(data: *const ()) {
    Rc::<Task>::increment_strong_count(data.cast());
    Rc::<Task>::from_raw(data.cast()).wake();
}

unsafe fn drop(data: *const ()) {
    Rc::<Task>::decrement_strong_count(data.cast());
}

const VTABLE: &RawWakerVTable = &RawWakerVTable::new(clone, wake, wake_by_ref, drop);

/// Converts a `Task` into a `Waker`.
///
/// When the returned `Waker` is woken, the task will be enqueued to be polled
/// by the executor.
#[must_use]
pub fn into_waker(task: Rc<Task>) -> Waker {
    let data = Rc::into_raw(task).cast();
    let raw_waker = RawWaker::new(data, VTABLE);
    unsafe { Waker::from_raw(raw_waker) }
}
