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
//! A _trap_ is an event handling mechanism in the shell. The user can prepare a
//! trap by using the `trap` built-in so that the shell runs desired commands in
//! response to a specific event happening later. This module contains functions
//! to detect the occurrence of events and run trap commands accordingly.
//!
//! # Related items
//!
//! Traps set by the user are stored in a [trap set](TrapSet) provided by the
//! [`yash_env`] crate.
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
//! TODO: Document

#[cfg(doc)]
use yash_env::trap::TrapSet;
#[cfg(doc)]
use yash_env::Env;

mod signal;
pub use signal::run_traps_for_caught_signals;

mod exit;
pub use exit::run_exit_trap;
