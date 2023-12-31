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
//! The **`trap`** built-in sets or prints [traps](yash_env::trap).
//!
//! # Synopsis
//!
//! ```sh
//! trap
//! ```
//!
//! ```sh
//! trap [action] condition…
//! ```
//!
//! # Description
//!
//! When the built-in is invoked with no operands, it prints the currently
//! configured traps in the format `trap -- action condition` where *action* and
//! *condition* are properly quoted so that the output can be read by the shell
//! to restore the traps.
//!
//! When a [subshell](yash_env::subshell) is entered, traps other than
//! `Action::Ignore` are reset to the default action. This behavior would make
//! it impossible to save the current traps by using a command substitution as
//! in `traps=$(trap)`. To avoid this, when the built-in is invoked in a
//! subshell and no traps have been modified in the subshell, it prints the
//! traps that were configured in the parent shell.
//!
//! When operands are given, the built-in sets the trap specified by *action*
//! and *condition*. When there are more than one *condition*, the built-in sets
//! the same *action* for all of them.
//!
//! # Options
//!
//! None.
//!
//! (TODO: `-p` option)
//!
//! # Operands
//!
//! An ***action*** specifies what to do when the trap condition is met. It may
//! be one of the following:
//!
//! - `-` (hyphen) resets the trap to the default action.
//! - An empty string ignores the trap.
//! - Any other string is treated as a command to execute.
//!
//! The *action* may be omitted if the first *condition* is a non-negative
//! decimal integer. In this case, the built-in resets the trap to the default
//! action.
//!
//! A ***condition*** specifies when the action is triggered. It may be one of
//! the following:
//!
//! - A symbolic name of a signal without the `SIG` prefix (e.g. `INT`, `QUIT`,
//!   `TERM`)
//!     - (TODO: Support names with `SIG` prefix)
//!     - (TODO: Support non-uppercase names)
//! - A positive decimal integer representing a signal number
//! - The number `0` or the symbolic name `EXIT` representing the termination of
//!   the main shell process
//!     - This condition is not triggered when the shell exits due to a signal.
//!
//! # Errors
//!
//! Traps cannot be set to `SIGKILL` or `SIGSTOP`.
//!
//! Invalid *condition*s are reported with a non-zero exit status, but the
//! built-in does not set `Divert::Interrupt` in the result.
//!
//! If a non-interactive shell inherited `Action::Ignore` for a signal, the
//! action cannot be changed. However, in this implementation, this error is not
//! reported and does not affect the exit status of the built-in.
//!
//! # Exit status
//!
//! Zero if successful, non-zero if an error is reported.
//!
//! # Portability
//!
//! Portable scripts should specify signals in uppercase letters without the
//! `SIG` prefix. Specifying signals by numbers is discouraged as signal numbers
//! vary among systems.
//!
//! The result of setting a trap to `SIGKILL` or `SIGSTOP` is undefined by
//! POSIX.
//!
//! The mechanism for the built-in to print traps configured in the parent shell
//! may vary among shells. This implementation remembers the old traps in the
//! [`TrapSet`] when starting a subshell and prints them when the built-in is
//! invoked in the subshell. POSIX allows another scheme: When starting a
//! subshell, the shell checks if the subshell command contains only a single
//! invocation of the `trap` built-in, in which case the shell skips resetting
//! traps on the subshell entry so that the built-in can print the traps
//! configured in the parent shell. The check may be done by a simple literal
//! comparison, so you should not expect the shell to recognize complex
//! expressions such as `cmd=trap; traps=$($cmd)`.
//!
//! # Implementation notes
//!
//! The [`TrapSet`] remembers the traps that were configured in the parent shell
//! so that the built-in can print them when invoked in a subshell. Those traps
//! are cleared when the built-in modifies any trap in the subshell. See
//! [`TrapSet::enter_subshell`] and [`TrapSet::set_action`] for details.

use crate::common::report_error;
use crate::common::report_failure;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::to_single_message;
use std::borrow::Cow;
use std::fmt::Write;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::trap::Action;
use yash_env::trap::Condition;
use yash_env::trap::SetActionError;
use yash_env::trap::TrapSet;
use yash_env::Env;
use yash_quote::quoted;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

/// Interpretation of command line arguments that selects the behavior of the
/// `trap` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Print all traps
    PrintAll,

    /// Set an action for one or more conditions
    SetAction {
        action: Action,
        conditions: Vec<(Condition, Field)>,
    },
}

pub mod syntax;

/// Returns a string that represents the currently configured traps.
///
/// The returned string is the whole output of the `trap` built-in
/// without operands, including the trailing newline.
pub fn display_traps(traps: &TrapSet) -> String {
    let mut output = String::new();
    for (cond, current, parent) in traps {
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
    output
}

impl Command {
    /// Executes the trap built-in.
    ///
    /// If successful, returns a string that should be printed to the standard
    /// output. On failure, returns a non-empty list of errors.
    pub fn execute(self, env: &mut Env) -> Result<String, Vec<(SetActionError, Condition, Field)>> {
        match self {
            Self::PrintAll => Ok(display_traps(&env.traps)),

            Self::SetAction { action, conditions } => {
                let mut errors = Vec::new();
                for (cond, field) in conditions {
                    if let Err(error) = env.traps.set_action(
                        &mut env.system,
                        cond,
                        action.clone(),
                        field.origin.clone(),
                        // TODO an interactive shell can override originally ignored traps
                        false,
                    ) {
                        errors.push((error, cond, field));
                    }
                }

                if errors.is_empty() {
                    Ok(String::new())
                } else {
                    Err(errors)
                }
            }
        }
    }
}

/// Wrapper for creating an error message for a trap setting error
#[derive(Clone, Debug, Eq, PartialEq)]
struct Error((SetActionError, Condition, Field));

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        "cannot update trap".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let (error, cond, field) = &self.0;
        let label = match error {
            SetActionError::InitiallyIgnored => todo!(),
            SetActionError::SIGKILL => "SIGKILL cannot be caught or ignored".into(),
            SetActionError::SIGSTOP => "SIGSTOP cannot be caught or ignored".into(),
            SetActionError::SystemError(errno) => {
                format!("system error updating trap for `{cond}`: {errno}").into()
            }
        };
        Annotation::new(AnnotationType::Error, label, &field.origin)
    }
}

/// Entry point for executing the `trap` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    let (options, operands) = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };

    let command = match syntax::interpret(options, operands) {
        Ok(command) => command,
        Err(errors) => {
            let is_soft_failure = errors
                .iter()
                .all(|e| matches!(e, syntax::Error::UnknownCondition(_)));
            let message = to_single_message(&errors).unwrap();
            let mut result = report_error(env, message).await;
            if is_soft_failure {
                result = crate::Result::from(ExitStatus::FAILURE);
            }
            return result;
        }
    };

    match command.execute(env) {
        Ok(output) => crate::common::output(env, &output).await,
        Err(mut errors) => {
            // For now, we ignore the InitiallyIgnored error since it is not
            // required by POSIX.
            errors.retain(|(error, _, _)| *error != SetActionError::InitiallyIgnored);
            let errors = errors.into_iter().map(Error).collect::<Vec<_>>();
            match to_single_message(&{ errors }) {
                None => crate::Result::default(),
                Some(message) => report_failure(env, message).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::Result;
    use futures_util::future::FutureExt;
    use std::ops::ControlFlow::{Break, Continue};
    use std::rc::Rc;
    use yash_env::io::Fd;
    use yash_env::semantics::Divert;
    use yash_env::stack::Builtin;
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
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        }));
        let args = Field::dummies(["echo", "INT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let actual_result = main(&mut env, vec![]).now_or_never().unwrap();
        let expected_result = Result::with_exit_status_and_divert(
            ExitStatus::FAILURE,
            Break(Divert::Interrupt(None)),
        );
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn unknown_condition() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        }));
        let args = Field::dummies(["echo", "FOOBAR"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::FAILURE, Continue(()));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn missing_condition() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        }));
        let args = Field::dummies(["echo"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result =
            Result::with_exit_status_and_divert(ExitStatus::ERROR, Break(Divert::Interrupt(None)));
        assert_eq!(actual_result, expected_result);
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn ignoring_initially_ignored_signal() {
        let mut system = VirtualSystem::new();
        system
            .current_process_mut()
            .set_signal_handling(Signal::SIGINT, SignalHandling::Ignore);
        let mut env = Env::with_system(Box::new(system.clone()));
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        }));
        let args = Field::dummies(["echo", "INT"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stderr(&system.state, |stderr| assert_eq!(stderr, ""));
        assert_eq!(
            system.current_process().signal_handling(Signal::SIGINT),
            SignalHandling::Ignore
        );
    }

    #[test]
    fn trying_to_trap_sigkill() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        }));
        let args = Field::dummies(["echo", "KILL"]);

        let actual_result = main(&mut env, args).now_or_never().unwrap();
        let expected_result = Result::with_exit_status_and_divert(
            ExitStatus::FAILURE,
            Break(Divert::Interrupt(None)),
        );
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
