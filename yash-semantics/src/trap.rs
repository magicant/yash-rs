// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Handling traps.
//!
//! _Traps_ are an event-handling mechanism in the shell. The user can prepare a
//! trap by using the trap built-in so that the shell performs a desired action
//! in response to a specific condition that may occur later. This module
//! contains functions to detect the conditions and run trap actions
//! accordingly.
//!
//! # Related items
//!
//! Traps set by the user are stored in a [trap set](TrapSet) implemented in the
//! [`yash_env::trap`] module.
//! The `trap` built-in is implemented in the `yash_builtin` crate.
//!
//! # Signal traps
//!
//! When an [environment](Env) catches a signal with a function like
//! [`wait_for_signals`](Env::wait_for_signals) and
//! [`poll_signals`](Env::poll_signals), the signal is stored as "pending" in
//! the trap set in the environment. The [`run_traps_for_caught_signals`]
//! function consumes those pending signals and runs the corresponding commands
//! specified in the trap set. The function is called periodically as the shell
//! executes main commands; roughly before and after each command.
//!
//! # Non-signal traps
//!
//! The EXIT trap is executed when the shell exits normally, by running the exit
//! built-in or reaching the end of the script. The [`run_exit_trap`] function,
//! which should be called before exiting, runs the trap.

use crate::Runtime;
use crate::read_eval_loop;
use std::cell::RefCell;
use std::ops::ControlFlow::Break;
use std::rc::Rc;
use yash_env::Env;
use yash_env::semantics::Divert;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::trap::Condition;
#[cfg(doc)]
use yash_env::trap::TrapSet;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Location;
use yash_syntax::source::Source;

/// Helper function for running a trap action.
///
/// This function pushes a temporary frame `Frame::Trap(cond)` to the
/// environment stack and runs the trap action by parsing the `code` with the
/// given `origin`. The exit status of the trap action does not affect the exit
/// status of the current environment except when the trap action is interrupted
/// with `Result::Break(Divert::Interrupt(_))`. In that case, the exit status of
/// the trap action is left as is in the environment.
///
/// Other variants of `Result::Break(Divert::…)` are simply passed on to the
/// caller. (It is unclear whether POSIX intends to require this behavior for
/// `Divert::Break` and `Divert::Continue`, but it is implemented this way for
/// simplicity. The exit status section of the POSIX return built-in
/// specification mentions the intended behavior for the `Divert::Return` case,
/// implying that the diversion should be passed on to the caller.)
async fn run_trap<S: Runtime + 'static>(
    env: &mut Env<S>,
    cond: Condition,
    code: Rc<str>,
    origin: Location,
) -> Result {
    let condition = cond.to_string(&env.system).into_owned();
    let mut lexer = Lexer::from_memory(&code, Source::Trap { condition, origin });
    let mut env = env.push_frame(Frame::Trap(cond));

    let previous_exit_status = env.exit_status;

    // Boxing needed for recursion
    let mut result = Box::pin(read_eval_loop(&RefCell::new(&mut env), &mut lexer)).await;

    if let Break(Divert::Interrupt(ref mut exit_status)) = result {
        if let Some(exit_status) = exit_status {
            // Propagate the exit status of the error that interrupted the trap
            *exit_status = env.exit_status
        }
    } else {
        // Restore the exit status of the calling context
        env.exit_status = previous_exit_status
    }

    result
}

mod signal;
pub use signal::run_trap_if_caught;
pub use signal::run_traps_for_caught_signals;

mod exit;
pub use exit::run_exit_trap;
