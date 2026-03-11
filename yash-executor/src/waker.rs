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
    unsafe {
        Rc::<Task>::increment_strong_count(data.cast());
        RawWaker::new(data, VTABLE)
    }
}

unsafe fn wake(data: *const ()) {
    unsafe {
        Rc::<Task>::from_raw(data.cast()).wake();
    }
}

unsafe fn wake_by_ref(data: *const ()) {
    unsafe {
        Rc::<Task>::increment_strong_count(data.cast());
        Rc::<Task>::from_raw(data.cast()).wake();
    }
}

unsafe fn drop(data: *const ()) {
    unsafe {
        Rc::<Task>::decrement_strong_count(data.cast());
    }
}

const VTABLE: &RawWakerVTable = &RawWakerVTable::new(clone, wake, wake_by_ref, drop);

/// Converts a `Task` into a `Waker`.
///
/// When the returned `Waker` is woken, the task will be enqueued to be polled
/// by the executor.
#[must_use]
pub(crate) fn into_waker(task: Rc<Task>) -> Waker {
    let data = Rc::into_raw(task).cast();
    let raw_waker = RawWaker::new(data, VTABLE);
    unsafe { Waker::from_raw(raw_waker) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Task;
    use alloc::boxed::Box;
    use alloc::rc::Weak;
    use core::cell::RefCell;

    fn dummy_task() -> Rc<Task<'static>> {
        Rc::new(Task {
            executor: Weak::new(),
            future: RefCell::new(Some(Box::pin(async { unreachable!() }))),
        })
    }

    #[test]
    fn clone_increments_strong_count() {
        let task = dummy_task();
        let waker = into_waker(Rc::clone(&task));
        assert_eq!(Rc::strong_count(&task), 2);

        let clone = waker.clone();
        assert_eq!(Rc::strong_count(&task), 3);

        core::mem::drop(clone);
        assert_eq!(Rc::strong_count(&task), 2);

        core::mem::drop(waker);
        assert_eq!(Rc::strong_count(&task), 1);
    }

    #[test]
    fn wake_consumes_one_ref() {
        let task = dummy_task();
        let waker = into_waker(Rc::clone(&task));
        assert_eq!(Rc::strong_count(&task), 2);

        waker.wake();

        assert_eq!(Rc::strong_count(&task), 1);
    }

    #[test]
    fn wake_by_ref_does_not_consume_ref() {
        let task = dummy_task();
        let waker = into_waker(Rc::clone(&task));
        assert_eq!(Rc::strong_count(&task), 2);

        waker.wake_by_ref();

        assert_eq!(Rc::strong_count(&task), 2);

        core::mem::drop(waker);
        assert_eq!(Rc::strong_count(&task), 1);
    }

    #[test]
    fn drop_decrements_strong_count() {
        let task = dummy_task();
        let waker = into_waker(Rc::clone(&task));
        assert_eq!(Rc::strong_count(&task), 2);

        core::mem::drop(waker);

        assert_eq!(Rc::strong_count(&task), 1);
    }
}
