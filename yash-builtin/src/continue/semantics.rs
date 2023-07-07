// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Main semantics of the continue built-in
//!
//! Some items in this module are shared with the break built-in.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::stack::Stack;

pub use crate::r#break::semantics::{Error, Result};

/// Computes the result of the continue built-in.
pub fn run(stack: &Stack, max_count: NonZeroUsize) -> Result {
    let count = stack.loop_count(max_count.get());
    if count == 0 {
        return Err(Error::NotInLoop);
    }

    Ok(crate::Result::with_exit_status_and_divert(
        ExitStatus::SUCCESS,
        ControlFlow::Break(Divert::Continue { count: count - 1 }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::semantics::Field;
    use yash_env::stack::Frame;
    use yash_env::stack::StackFrameGuard;

    fn push_continue_builtin(stack: &mut Stack) -> StackFrameGuard<'_> {
        stack.push(Frame::Builtin {
            name: Field::dummy("continue"),
            is_special: true,
        })
    }

    #[test]
    fn count_fewer_than_loops() {
        let mut stack = Stack::default();
        let mut stack = push_continue_builtin(&mut stack);
        let mut stack = stack.push(Frame::Loop);
        let stack = stack.push(Frame::Loop);
        let count = NonZeroUsize::new(1).unwrap();

        let result = run(&stack, count);
        assert_eq!(
            result,
            Ok(crate::Result::with_exit_status_and_divert(
                ExitStatus::SUCCESS,
                ControlFlow::Break(Divert::Continue { count: 0 }),
            ))
        );
    }

    #[test]
    fn count_equal_to_loops() {
        let mut stack = Stack::default();
        let mut stack = push_continue_builtin(&mut stack);
        let stack = stack.push(Frame::Loop);
        let count = NonZeroUsize::new(1).unwrap();

        let result = run(&stack, count);
        assert_eq!(
            result,
            Ok(crate::Result::with_exit_status_and_divert(
                ExitStatus::SUCCESS,
                ControlFlow::Break(Divert::Continue { count: 0 }),
            ))
        );
    }

    #[test]
    fn count_more_than_loops() {
        let mut stack = Stack::default();
        let mut stack = push_continue_builtin(&mut stack);
        let mut stack = stack.push(Frame::Loop);
        let stack = stack.push(Frame::Loop);
        let count = NonZeroUsize::new(3).unwrap();

        let result = run(&stack, count);
        assert_eq!(
            result,
            Ok(crate::Result::with_exit_status_and_divert(
                ExitStatus::SUCCESS,
                ControlFlow::Break(Divert::Continue { count: 1 }),
            ))
        );
    }

    #[test]
    fn not_in_loop() {
        let mut stack = Stack::default();
        let stack = push_continue_builtin(&mut stack);
        let count = NonZeroUsize::new(1).unwrap();
        let result = run(&stack, count);
        assert_eq!(result, Err(Error::NotInLoop));
    }
}
