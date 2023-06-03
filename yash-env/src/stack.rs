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

//! Runtime execution context stack
//!
//! The ["stack"](Stack) traces the state of execution context at runtime.
//! For example, when entering a subshell, the runner pushes `Frame::Subshell`
//! to the stack. By examining the stack, commands executed in the subshell can
//! detect that they are inside the subshell.
//!
//! This module provides guards to ensure stack frames are pushed and popped
//! correctly. The push function returns a guard that will pop the frame when
//! dropped. Implementing `Deref` and `DerefMut`, the guard allows access to the
//! borrowed stack or environment.
//!
//! [`Stack::push`] returns a [`StackFrameGuard`] that allows re-borrowing the
//! `Stack`. [`Env::push_frame`] returns a [`EnvFrameGuard`] that implements
//! `DerefMut<Target = Env>`.

use crate::semantics::ExitStatus;
use crate::semantics::Field;
use crate::Env;
use std::ops::Deref;
use std::ops::DerefMut;

/// Element of runtime execution context stack
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Frame {
    /// For, while, or until loop
    Loop,

    /// Subshell
    Subshell,

    /// Context where the `ErrExit` [option](crate::option::Option) is ignored
    ///
    /// This frame is pushed when executing negated commands, the condition part
    /// of and-or lists and the `if`, `while`, and `until` commands.
    Condition,

    /// Built-in utility
    Builtin {
        /// Name of the built-in
        name: Field,

        /// Whether the utility acts as a special built-in
        ///
        /// This value determines whether an error in the built-in interrupts
        /// the shell. This will be false if a special built-in is executed
        /// through the `command` built-in.
        is_special: bool,
    },

    /// Trap
    Trap {
        /// Condition that triggered the trap
        condition: crate::trap::Condition,
        /// Exit status saved just before entering the trap
        previous_exit_status: ExitStatus,
    },
    // TODO dot script, eval
}

/// Runtime execution context stack
///
/// You can access the inner vector of the stack via the `Deref` implementation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Stack {
    inner: Vec<Frame>,
}

impl Deref for Stack {
    type Target = Vec<Frame>;
    fn deref(&self) -> &Vec<Frame> {
        &self.inner
    }
}

impl From<Vec<Frame>> for Stack {
    fn from(vec: Vec<Frame>) -> Self {
        Stack { inner: vec }
    }
}

impl From<Stack> for Vec<Frame> {
    fn from(vec: Stack) -> Self {
        vec.inner
    }
}

/// RAII-style guard that makes sure a stack frame is popped properly
///
/// The guard object is created by [`Stack::push`].
#[derive(Debug)]
#[must_use = "The frame is popped when the guard is dropped"]
pub struct StackFrameGuard<'a> {
    stack: &'a mut Stack,
}

impl Stack {
    /// Pushes a new frame to the stack.
    ///
    /// This function returns a frame guard that will pop the frame when dropped.
    #[inline]
    pub fn push(&mut self, frame: Frame) -> StackFrameGuard<'_> {
        self.inner.push(frame);
        StackFrameGuard { stack: self }
    }

    /// Pops the topmost frame from the stack
    #[inline]
    pub fn pop(guard: StackFrameGuard<'_>) -> Frame {
        let frame = guard.stack.inner.pop().unwrap();
        std::mem::forget(guard);
        frame
    }

    /// Returns the number of enclosing loops.
    ///
    /// This function returns the number of lexically enclosing `for`, `while`,
    /// and `until` loops in the current execution environment. That is, the
    /// result is the count of `Frame::Loop`s pushed after the last
    /// `Frame::Subshell`.
    ///
    /// The function stops counting when `max_count` is reached. The parameter
    /// is useful if you don't have to count more than a specific number.
    /// Pass `usize::MAX` to count all loops.
    #[must_use]
    pub fn loop_count(&self, max_count: usize) -> usize {
        self.inner
            .iter()
            .rev()
            .take_while(|&frame| frame != &Frame::Subshell)
            .filter(|&frame| frame == &Frame::Loop)
            .take(max_count)
            .count()
    }
}

/// When the guard is dropped, the stack frame that was pushed when creating the
/// guard is popped.
impl Drop for StackFrameGuard<'_> {
    fn drop(&mut self) {
        self.stack.inner.pop().unwrap();
    }
}

impl Deref for StackFrameGuard<'_> {
    type Target = Stack;
    fn deref(&self) -> &Stack {
        self.stack
    }
}

impl DerefMut for StackFrameGuard<'_> {
    fn deref_mut(&mut self) -> &mut Stack {
        self.stack
    }
}

/// RAII-style guard that makes sure a stack frame is popped properly
///
/// The guard object is created by [`Env::push_frame`].
#[derive(Debug)]
#[must_use = "The frame is popped when the guard is dropped"]
pub struct EnvFrameGuard<'a> {
    env: &'a mut Env,
}

impl Env {
    /// Pushes a new frame to the runtime execution context stack.
    ///
    /// This function is equivalent to `self.stack.push(frame)`, but returns an
    /// `EnvFrameGuard` that allows re-borrowing the `Env`.
    #[inline]
    pub fn push_frame(&mut self, frame: Frame) -> EnvFrameGuard<'_> {
        self.stack.inner.push(frame);
        EnvFrameGuard { env: self }
    }

    /// Pops the topmost frame from the runtime execution context stack.
    #[inline]
    pub fn pop_frame(guard: EnvFrameGuard<'_>) -> Frame {
        let frame = guard.env.stack.inner.pop().unwrap();
        std::mem::forget(guard);
        frame
    }
}

/// When the guard is dropped, the stack frame that was pushed when creating the
/// guard is popped.
impl Drop for EnvFrameGuard<'_> {
    fn drop(&mut self) {
        self.env.stack.inner.pop().unwrap();
    }
}

impl Deref for EnvFrameGuard<'_> {
    type Target = Env;
    fn deref(&self) -> &Env {
        self.env
    }
}

impl DerefMut for EnvFrameGuard<'_> {
    fn deref_mut(&mut self) -> &mut Env {
        self.env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_count_empty() {
        let stack = Stack::default();
        assert_eq!(stack.loop_count(usize::MAX), 0);
    }

    #[test]
    fn loop_count_with_non_loop_frames() {
        let mut stack = Stack::default();
        let mut stack = stack.push(Frame::Builtin {
            name: Field::dummy(""),
            is_special: false,
        });
        assert_eq!(stack.loop_count(usize::MAX), 0);
        let stack = stack.push(Frame::Condition);
        assert_eq!(stack.loop_count(usize::MAX), 0);
    }

    #[test]
    fn loop_count_with_one_loop() {
        let mut stack = Stack::default();
        let mut stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(usize::MAX), 1);
        let stack = stack.push(Frame::Condition);
        assert_eq!(stack.loop_count(usize::MAX), 1);
    }

    #[test]
    fn loop_count_with_two_loops() {
        let mut stack = Stack::default();
        let mut stack = stack.push(Frame::Loop);
        let mut stack = stack.push(Frame::Condition);
        let mut stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(usize::MAX), 2);
        let stack = stack.push(Frame::Condition);
        assert_eq!(stack.loop_count(usize::MAX), 2);
    }

    #[test]
    fn loop_count_with_subshells() {
        let mut stack = Stack::default();
        let mut stack = stack.push(Frame::Loop);
        let mut stack = stack.push(Frame::Subshell);
        assert_eq!(stack.loop_count(usize::MAX), 0);
        let mut stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(usize::MAX), 1);
        let mut stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(usize::MAX), 2);
        let mut stack = stack.push(Frame::Subshell);
        assert_eq!(stack.loop_count(usize::MAX), 0);
        let stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(usize::MAX), 1);
    }

    #[test]
    fn loop_count_with_small_max_count() {
        let mut stack = Stack::default();
        let mut stack = stack.push(Frame::Loop);
        let mut stack = stack.push(Frame::Condition);
        let mut stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(usize::MAX), 2);
        assert_eq!(stack.loop_count(3), 2);
        assert_eq!(stack.loop_count(2), 2);
        assert_eq!(stack.loop_count(1), 1);
        assert_eq!(stack.loop_count(0), 0);

        let stack = stack.push(Frame::Loop);
        assert_eq!(stack.loop_count(4), 3);
        assert_eq!(stack.loop_count(3), 3);
        assert_eq!(stack.loop_count(2), 2);
    }
}
