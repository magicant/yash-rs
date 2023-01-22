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
//! TODO: Document

use crate::ReadEvalLoop;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::trap::Action;
#[cfg(doc)]
use yash_env::trap::TrapSet;
use yash_env::Env;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;

fn in_trap(env: &Env) -> bool {
    env.stack
        .iter()
        .rev()
        .take_while(|frame| **frame != Frame::Subshell)
        .any(|frame| *frame == Frame::Trap)
}

/// Runs trap commands for signals that have been caught.
///
/// This function resets the `pending` flag of caught signals by calling
/// [`TrapSet::take_caught_signal`]. See the [module doc](self) for more
/// details.
///
/// If we are already running a trap, this function does not run any traps to
/// prevent unintended behavior of trap actions. Most shell script writers do
/// not care for the reentrance of trap actions, so we should not assume they
/// are reentrant. As an exception, this function does run traps in a subshell
/// executed in a trap.
pub async fn run_traps_for_caught_signals(env: &mut Env) -> Result {
    env.poll_signals();

    if in_trap(env) {
        // Do not run a trap action while running another
        return Continue(());
    }

    while let Some((signal, state)) = env.traps.take_caught_signal() {
        let code = if let Action::Command(command) = &state.action {
            command.clone()
        } else {
            continue;
        };
        let condition = signal.to_string();
        let origin = state.origin.clone();
        let mut lexer = Lexer::from_memory(&code, Source::Trap { condition, origin });
        let mut env = env.push_frame(Frame::Trap);
        let previous_exit_status = env.exit_status;
        // Boxing needed for recursion
        let future: Pin<Box<dyn Future<Output = Result>>> =
            Box::pin(ReadEvalLoop::new(&mut env, &mut lexer).run());
        future.await?;
        env.exit_status = previous_exit_status;
    }

    Continue(())
}

mod exit;
pub use exit::run_exit_trap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::pin::Pin;
    use yash_env::builtin::Builtin;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::trap::Action;
    use yash_env::trap::Signal;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    fn signal_env() -> (Env, VirtualSystem) {
        let system = VirtualSystem::default();
        let mut env = Env::with_system(Box::new(system.clone()));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGINT,
                Action::Command("echo trapped".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGUSR1,
                Action::Command("return 56".into()),
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
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn running_trap() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGINT);
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "trapped\n"));
    }

    #[test]
    fn no_reentrance() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGINT);
        let mut env = env.push_frame(Frame::Trap);
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
    }

    // TODO allow_reentrance_in_exit_trap

    #[test]
    fn allow_reentrance_in_subshell() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGINT);
        let mut env = env.push_frame(Frame::Trap);
        let mut env = env.push_frame(Frame::Subshell);
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "trapped\n"));
    }

    #[test]
    fn stack_frame_in_trap_action() {
        fn execute(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_matches!(&env.stack[0], Frame::Trap);
                Default::default()
            })
        }
        let system = VirtualSystem::default();
        let mut env = Env::with_system(Box::new(system.clone()));
        let r#type = yash_env::builtin::Type::Intrinsic;
        env.builtins.insert("check", Builtin { r#type, execute });
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGINT,
                Action::Command("check".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        raise_signal(&system, Signal::SIGINT);
        let _ = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
    }

    #[test]
    fn exit_status_is_restored_after_running_trap() {
        let (mut env, system) = signal_env();
        env.exit_status = ExitStatus(42);
        raise_signal(&system, Signal::SIGINT);
        let _ = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(env.exit_status, ExitStatus(42));
    }

    #[test]
    fn exit_status_inside_trap() {
        let (mut env, system) = signal_env();
        for signal in [Signal::SIGUSR1, Signal::SIGUSR2] {
            env.traps
                .set_action(
                    &mut env.system,
                    signal,
                    Action::Command("echo $?; echo $?".into()),
                    Location::dummy(""),
                    false,
                )
                .unwrap();
        }
        env.exit_status = ExitStatus(123);
        raise_signal(&system, Signal::SIGUSR1);
        raise_signal(&system, Signal::SIGUSR2);
        let _ = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, "123\n0\n123\n0\n")
        });
    }

    #[test]
    fn exit_from_trap() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGUSR1);
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Break(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(56));
    }

    // TODO Should we suppress return/break/continue from trap?
    // // TODO exit status on return/exit from trap
}
