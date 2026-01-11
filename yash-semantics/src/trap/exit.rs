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

use super::run_trap;
use std::rc::Rc;
use yash_env::Env;
use crate::Runtime;
use yash_env::trap::Action;
use yash_env::trap::Condition;
use yash_env::trap::Origin;

/// Executes the EXIT trap.
///
/// If the EXIT trap is not set in the environment, this function does nothing.
/// Otherwise, this function executes the EXIT trap.
///
/// The exit status of the trap is ignored: The exit status is saved on entry to
/// this function and restored when finished. However, if the trap terminates
/// with a `Break(divert)` where `divert.exit_status()` is `Some` exit status,
/// that exit status is set to `env.exit_status`.
pub async fn run_exit_trap<S: Runtime + 'static>(env: &mut Env<S>) {
    let Some(state) = env.traps.get_state(Condition::Exit).0 else {
        return;
    };
    let Action::Command(command) = &state.action else {
        return;
    };

    let command = Rc::clone(command);
    let origin = match &state.origin {
        Origin::Inherited | Origin::Subshell => panic!("user-defined trap must have origin"),
        Origin::User(location) => location.clone(),
    };
    let result = run_trap(env, Condition::Exit, command, origin).await;
    env.apply_result(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::exit_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::pin::Pin;
    use yash_env::builtin::Builtin;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::stack::Frame;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env_test_helper::assert_stdout;
    use yash_syntax::source::Location;

    #[test]
    fn does_nothing_if_exit_trap_is_not_set() {
        let mut env = Env::new_virtual();
        run_exit_trap(&mut env).now_or_never().unwrap();
    }

    #[test]
    fn runs_exit_trap() {
        let system = VirtualSystem::new();
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
            env: &mut Env<VirtualSystem>,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_matches!(&env.stack[0], Frame::Trap(Condition::Exit));
                Default::default()
            })
        }
        let mut env = Env::new_virtual();
        env.builtins.insert(
            "check",
            Builtin::new(yash_env::builtin::Type::Mandatory, execute),
        );
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
        let system = VirtualSystem::new();
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
                Action::Command("return -n 53; return".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();

        run_exit_trap(&mut env).now_or_never().unwrap();
        // the exit status is restored
        assert_eq!(env.exit_status, ExitStatus::default());
    }

    #[test]
    fn exit_from_trap_with_specified_exit_status() {
        let mut env = Env::new_virtual();
        env.builtins.insert("exit", exit_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("exit 31".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();

        run_exit_trap(&mut env).now_or_never().unwrap();
        assert_eq!(env.exit_status, ExitStatus(31));
    }

    #[test]
    fn exit_from_trap_without_specified_exit_status() {
        let mut env = Env::new_virtual();
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("exit", exit_builtin());
        env.traps
            .set_action(
                &mut env.system,
                Condition::Exit,
                Action::Command("echo; exit".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        env.exit_status = ExitStatus(72);

        run_exit_trap(&mut env).now_or_never().unwrap();
        assert_eq!(env.exit_status, ExitStatus(72));
    }
}
