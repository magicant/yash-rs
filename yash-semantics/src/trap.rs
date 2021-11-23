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
//! Traps set by the user are stored in a [trap set](yash_env::trap::TrapSet)
//! provided by the [`yash_env`] crate.
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
//! TODO: Not yet implemented

use crate::read_eval_loop_boxed;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::Result;
use yash_env::trap::Trap;
#[cfg(doc)]
use yash_env::trap::TrapSet;
use yash_env::Env;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;

/// Runs trap commands for signals that have been caught.
///
/// This function resets the `pending` flag of caught signals by calling
/// [`TrapSet::take_caught_signal`]. See the [module doc](self) for more
/// details.
pub async fn run_traps_for_caught_signals(env: &mut Env) -> Result {
    env.poll_signals();

    // TODO Prevent running traps while running another

    while let Some((signal, state)) = env.traps.take_caught_signal() {
        let code = if let Trap::Command(command) = &state.action {
            command.clone()
        } else {
            continue;
        };
        let condition = signal.to_string();
        let origin = state.origin.clone();
        let mut lexer = Lexer::from_memory(&code, Source::Trap { condition, origin });
        let previous_exit_status = env.exit_status;
        // TODO Update control flow stack
        read_eval_loop_boxed(env, &mut lexer).await?;
        env.exit_status = previous_exit_status;
    }

    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_executor::block_on;
    use std::ops::ControlFlow::Break;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::trap::Signal;
    use yash_env::trap::Trap;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    fn signal_env() -> (Env, VirtualSystem) {
        let system = VirtualSystem::default();
        let mut env = Env::with_system(Box::new(system.clone()));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.traps
            .set_trap(
                &mut env.system,
                Signal::SIGINT,
                Trap::Command("echo trapped".to_string()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        env.traps
            .set_trap(
                &mut env.system,
                Signal::SIGUSR1,
                Trap::Command("return 56".to_string()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        (env, system)
    }

    fn raise_signal(system: &VirtualSystem, signal: Signal) {
        system
            .state
            .borrow_mut()
            .processes
            .get_mut(&system.process_id)
            .unwrap()
            .raise_signal(signal);
    }

    #[test]
    fn nothing_to_do_without_signals_caught() {
        let (mut env, system) = signal_env();
        let result = block_on(run_traps_for_caught_signals(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(
            system
                .state
                .borrow()
                .file_system
                .get("/dev/stdout")
                .unwrap()
                .borrow()
                .content,
            []
        );
    }

    #[test]
    fn running_trap() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGINT);
        let result = block_on(run_traps_for_caught_signals(&mut env));
        assert_eq!(result, Continue(()));
        assert_eq!(
            system
                .state
                .borrow()
                .file_system
                .get("/dev/stdout")
                .unwrap()
                .borrow()
                .content,
            b"trapped\n"
        );
    }

    #[test]
    fn exit_status_is_restored_after_running_trap() {
        let (mut env, system) = signal_env();
        env.exit_status = ExitStatus(42);
        raise_signal(&system, Signal::SIGINT);
        let _ = block_on(run_traps_for_caught_signals(&mut env));
        assert_eq!(env.exit_status, ExitStatus(42));
    }

    #[test]
    fn exit_from_trap() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGUSR1);
        let result = block_on(run_traps_for_caught_signals(&mut env));
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(56));
    }

    // TODO exit status on return/exit from trap
    // TODO $? inside trap
}
