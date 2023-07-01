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

//! Trap built-in.
//!
//! TODO Elaborate

use crate::common::arg::parse_arguments;
use crate::common::arg::Mode;
use crate::common::print_error_message;
use crate::common::Print;
use std::fmt::Write;
use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::trap::Action;
use yash_env::trap::Condition;
use yash_env::Env;
use yash_quote::quoted;

/// Prints the currently configured traps.
pub async fn print_traps(env: &mut Env) -> Result {
    let mut output = String::new();
    for (cond, current, parent) in env.traps.iter() {
        let trap = match (current, parent) {
            (Some(trap), _) => trap,
            (None, Some(trap)) => trap,
            (None, None) => continue,
        };
        let command = match &trap.action {
            Action::Default => continue,
            Action::Ignore => "",
            Action::Command(command) => command,
        };
        writeln!(output, "trap -- {} {}", quoted(command), cond).ok();
    }
    env.print(&output).await
}

/// Entry point for executing the `trap` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let (_options, mut operands) = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return print_error_message(env, &error).await,
    };

    match operands.len() {
        0 => return print_traps(env).await,
        2 => (),
        _ => return Result::new(ExitStatus::ERROR),
        // TODO Support full syntax
    }

    let Field { value, origin } = operands.remove(0);

    let cond = match operands[0].value.parse::<Condition>() {
        Ok(cond) => cond,
        // TODO Print error message for the unknown condition
        Err(_) => return Result::new(ExitStatus::FAILURE),
    };
    let action = match value.as_str() {
        "-" => Action::Default,
        "" => Action::Ignore,
        _ => Action::Command(value.into()),
    };

    match env
        .traps
        .set_action(&mut env.system, cond, action, origin, false)
    {
        Ok(()) => Result::new(ExitStatus::SUCCESS),
        // TODO Print error message
        Err(_) => Result::new(ExitStatus::ERROR),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use futures_util::future::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::io::Fd;
    use yash_env::semantics::Divert;
    use yash_env::stack::Frame;
    use yash_env::system::SignalHandling;
    use yash_env::trap::Signal;
    use yash_env::Env;
    use yash_env::VirtualSystem;

    #[test]
    fn setting_trap_to_ignore() {
        let system = Box::new(VirtualSystem::new());
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["", "USR1"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let process = &state.borrow().processes[&pid];
        assert_eq!(
            process.signal_handling(Signal::SIGUSR1),
            SignalHandling::Ignore
        );
    }

    #[test]
    fn setting_trap_to_command() {
        let system = Box::new(VirtualSystem::new());
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["echo", "USR2"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let process = &state.borrow().processes[&pid];
        assert_eq!(
            process.signal_handling(Signal::SIGUSR2),
            SignalHandling::Catch
        );
    }

    #[test]
    fn resetting_trap() {
        let system = Box::new(VirtualSystem::new());
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["-", "PIPE"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let process = &state.borrow().processes[&pid];
        assert_eq!(
            process.signal_handling(Signal::SIGPIPE),
            SignalHandling::Default
        );
    }

    #[test]
    fn printing_no_trap() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn printing_some_trap() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["echo", "INT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "trap -- echo INT\n"));
    }

    #[test]
    fn printing_some_traps() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["echo", "INT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["echo t", "TERM"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "trap -- echo INT\ntrap -- 'echo t' TERM\n")
        });
    }

    #[test]
    fn error_printing_traps() {
        let mut system = Box::new(VirtualSystem::new());
        system.current_process_mut().close_fd(Fd::STDOUT);
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        });
        let args = Field::dummies(["echo", "INT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let mut expected_result = Result::new(ExitStatus::FAILURE);
        expected_result.set_divert(Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn printing_traps_in_subshell() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["echo", "INT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["", "TERM"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        env.traps.enter_subshell(&mut env.system, false, false);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "trap -- echo INT\ntrap -- '' TERM\n")
        });
    }

    #[test]
    fn printing_traps_after_setting_in_subshell() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["echo", "INT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["", "TERM"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        env.traps.enter_subshell(&mut env.system, false, false);
        let args = Field::dummies(["ls", "QUIT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "trap -- ls QUIT\ntrap -- '' TERM\n")
        });
    }
}
