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

//! Running the EXIT trap

use crate::ReadEvalLoop;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::trap::Action;
use yash_env::trap::Condition;
use yash_env::Env;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;

/// Executes the EXIT trap.
///
/// If the EXIT trap is not set in the environment, this function does nothing.
/// Otherwise, this function executes the EXIT trap.
///
/// The exit status of the trap is ignored: The exit status is saved on entry to
/// this function and restored when finished. However, if the trap terminates
/// with a `Break(divert)` where `divert.exit_status()` is `Some` exit status,
/// that exit status is set to `env.exit_status`.
pub async fn run_exit_trap(env: &mut Env) {
    let Some(state) = env.traps.get_state(Condition::Exit).0 else { return; };
    let Action::Command(command) = &state.action else { return; };

    let command = Rc::clone(command);
    let condition = Condition::Exit.to_string();
    let origin = state.origin.clone();
    let mut lexer = Lexer::from_memory(&command, Source::Trap { condition, origin });
    let mut env = env.push_frame(Frame::Trap(Condition::Exit));
    let previous_exit_status = env.exit_status;
    // Boxing needed for recursion
    let future: Pin<Box<dyn Future<Output = Result>>> =
        Box::pin(ReadEvalLoop::new(&mut env, &mut lexer).run());
    let result = future.await;
    env.exit_status = previous_exit_status;
    env.apply_result(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use yash_env::builtin::Builtin;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_syntax::source::Location;

    #[test]
    fn does_nothing_if_exit_trap_is_not_set() {
        let mut env = Env::new_virtual();
        run_exit_trap(&mut env).now_or_never().unwrap();
    }

    #[test]
    fn runs_exit_trap() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("echo exit trap executed".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();

        run_exit_trap(&mut env).now_or_never().unwrap();
        assert_stdout(&state, |stdout| assert_eq!(stdout, "exit trap executed\n"));
    }

    #[test]
    fn stack_frame_in_trap_action() {
        fn execute(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_matches!(&env.stack[0], Frame::Trap(Condition::Exit));
                Default::default()
            })
        }
        let mut env = Env::new_virtual();
        let r#type = yash_env::builtin::Type::Intrinsic;
        env.builtins.insert("check", Builtin { r#type, execute });
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("check".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        run_exit_trap(&mut env).now_or_never().unwrap();
    }

    #[test]
    fn exit_status_is_restored_after_running_trap() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("return -n 123".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        env.exit_status = ExitStatus(42);

        run_exit_trap(&mut env).now_or_never().unwrap();
        assert_eq!(env.exit_status, ExitStatus(42));
    }

    #[test]
    fn exit_status_inside_trap() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.builtins.insert("echo", echo_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("echo $?; echo $?".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        env.exit_status = ExitStatus(123);

        run_exit_trap(&mut env).now_or_never().unwrap();
        assert_stdout(&state, |stdout| assert_eq!(stdout, "123\n0\n"));
    }

    #[test]
    fn return_from_trap() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("return 53".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();

        run_exit_trap(&mut env).now_or_never().unwrap();
        // the exit status is restored
        assert_eq!(env.exit_status, ExitStatus::default());
    }

    #[test]
    fn exit_from_trap() {
        fn execute(
            _env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                let mut result = yash_env::builtin::Result::default();
                result.set_divert(Result::Break(Divert::Exit(Some(ExitStatus(31)))));
                result
            })
        }
        let mut env = Env::new_virtual();
        let r#type = yash_env::builtin::Type::Intrinsic;
        env.builtins.insert("my_exit", Builtin { r#type, execute });
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("my_exit".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();

        run_exit_trap(&mut env).now_or_never().unwrap();
        assert_eq!(env.exit_status, ExitStatus(31));
    }
}
