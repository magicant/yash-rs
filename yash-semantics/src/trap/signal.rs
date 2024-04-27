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

//! Running signal traps

use super::run_trap;
use std::ops::ControlFlow::Continue;
use std::rc::Rc;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::trap::Action;
use yash_env::trap::Condition;
use yash_env::trap::Signal;
#[cfg(doc)]
use yash_env::trap::TrapSet;
use yash_env::Env;

/// Runs a trap action for a signal if it has been caught.
///
/// This function is similar to [`run_traps_for_caught_signals`], but this
/// function operates only on a single signal. Unlike
/// `run_traps_for_caught_signals`, this function runs a trap action even if
/// we are already running a trap action.
///
/// Returns `None` if the signal has not been caught. Otherwise, returns the
/// result of running the trap action.
#[must_use]
pub async fn run_trap_if_caught(env: &mut Env, signal: Signal) -> Option<Result> {
    let trap_state = env.traps.take_signal_if_caught(signal)?;
    let Action::Command(command) = &trap_state.action else {
        return None;
    };
    let code = Rc::clone(command);
    let origin = trap_state.origin.clone();
    Some(run_trap(env, signal.into(), code, origin).await)
}

fn in_trap(env: &Env) -> bool {
    env.stack
        .iter()
        .rev()
        .take_while(|frame| **frame != Frame::Subshell)
        .any(|frame| matches!(*frame, Frame::Trap(Condition::Signal(_))))
}

/// Runs trap commands for signals that have been caught.
///
/// This function resets the `pending` flag of caught signals by calling
/// [`TrapSet::take_caught_signal`]. See the [module doc](super) for more
/// details.
///
/// The exit status of trap actions does not affect the exit status of the
/// current environment except when the trap action is interrupted with
/// `Result::Break(Divert::Interrupt(_))`. In that case, the exit status of the
/// trap action is left as is in the environment.
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
        let Action::Command(command) = &state.action else {
            continue;
        };
        let code = Rc::clone(command);
        let origin = state.origin.clone();
        run_trap(env, signal.into(), code, origin).await?;
    }

    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::exit_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::future::Future;
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
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGINT,
                Action::Command("echo trapped".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        (env, system)
    }

    fn raise_signal(system: &VirtualSystem, signal: Signal) {
        let _ = system
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
        let mut env = env.push_frame(Frame::Trap(Condition::Signal(Signal::SIGTERM)));
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn allow_reentrance_in_exit_trap() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGINT);
        let mut env = env.push_frame(Frame::Trap(Condition::Exit));
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&system.state, |stdout| assert_eq!(stdout, "trapped\n"));
    }

    #[test]
    fn allow_reentrance_in_subshell() {
        let (mut env, system) = signal_env();
        raise_signal(&system, Signal::SIGINT);
        let mut env = env.push_frame(Frame::Trap(Condition::Signal(Signal::SIGTERM)));
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
                assert_matches!(
                    &env.stack[0],
                    Frame::Trap(Condition::Signal(Signal::SIGINT))
                );
                Default::default()
            })
        }
        let system = VirtualSystem::default();
        let mut env = Env::with_system(Box::new(system.clone()));
        let r#type = yash_env::builtin::Type::Mandatory;
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
    fn exit_from_trap_with_specified_exit_status() {
        let (mut env, system) = signal_env();
        env.builtins.insert("exit", exit_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGUSR1,
                Action::Command("echo; exit 56".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        raise_signal(&system, Signal::SIGUSR1);
        env.exit_status = ExitStatus(42);
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Break(Divert::Exit(Some(ExitStatus(56)))));
        assert_eq!(env.exit_status, ExitStatus(42));
    }

    #[test]
    fn exit_from_trap_without_specified_exit_status() {
        let (mut env, system) = signal_env();
        env.builtins.insert("exit", exit_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Signal::SIGUSR1,
                Action::Command("echo; exit".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        raise_signal(&system, Signal::SIGUSR1);
        env.exit_status = ExitStatus(42);
        let result = run_traps_for_caught_signals(&mut env)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Break(Divert::Exit(None)));
        assert_eq!(env.exit_status, ExitStatus(42));
    }

    // TODO Should we suppress return/break/continue from trap?
    // // TODO exit status on return/exit from trap
}
