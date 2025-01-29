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
//! trap [action] condition…
//! ```
//!
//! ```sh
//! trap [-p [condition…]]
//! ```
//!
//! # Description
//!
//! The `trap` built-in can be used to either set or print traps.
//! To set traps, pass an *action* and one or more *condition*s as operands.
//! To print the currently configured traps, invoke the built-in with no
//! operands or with the `-p` option.
//!
//! ## Setting traps
//!
//! When setting traps, the built-in sets the *action* for each *condition* in
//! the current shell environment. To set different actions for multiple
//! conditions, use multiple invocations of the built-in.
//!
//! ## Printing traps
//!
//! When the built-in is invoked with no operands, it prints the currently
//! configured traps in the format `trap -- action condition` where *action* and
//! *condition* are properly quoted so that the output can be read by the shell
//! to restore the traps. By default, the built-in prints traps that have
//! non-default actions. To print all traps, use the `-p` option with no
//! operands.
//!
//! When the `-p` option is used with one or more *condition*s, the built-in
//! prints the traps for the specified *condition*s.
//!
//! When a [subshell](yash_env::subshell) is entered, traps other than
//! `Action::Ignore` are reset to the default action. This behavior would make
//! it impossible to save the current traps by using a command substitution as
//! in `traps=$(trap)`. To make this work, when the built-in is invoked in a
//! subshell and no traps have been modified in the subshell, it prints the
//! traps that were configured in the parent shell.
//!
//! # Options
//!
//! The **`-p`** (**`--print`**) option prints the traps configured in the shell
//! environment.
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
//! In other shells, the `EXIT` condition may be triggered when the shell is
//! terminated by a signal.
//!
//! # Implementation notes
//!
//! The [`TrapSet`] remembers the traps that were configured in the parent shell
//! so that the built-in can print them when invoked in a subshell. Those traps
//! are cleared when the built-in modifies any trap in the subshell. See
//! [`TrapSet::enter_subshell`] and [`TrapSet::set_action`] for details.

mod cond;

pub use self::cond::CondSpec;
use crate::common::report_error;
use crate::common::report_failure;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::to_single_message;
use std::borrow::Cow;
use std::fmt::Write;
use thiserror::Error;
use yash_env::option::Option::Interactive;
use yash_env::option::State::On;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::SharedSystem;
use yash_env::trap::Action;
use yash_env::trap::SetActionError;
use yash_env::trap::SignalSystem;
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
        conditions: Vec<(CondSpec, Field)>,
    },
}

pub mod syntax;

/// Returns a string that represents the currently configured traps.
///
/// The returned string is the whole output of the `trap` built-in
/// without operands, including the trailing newline.
#[must_use]
pub fn display_traps<S: SignalSystem>(traps: &TrapSet, system: &S) -> String {
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
        let cond = cond.to_string(system);
        writeln!(output, "trap -- {} {}", quoted(command), cond).ok();
    }
    output
}

/// Cause of an error that may occur while executing the `trap` built-in
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ErrorCause {
    /// The specified condition is not supported.
    #[error("signal not supported on this system")]
    UnsupportedSignal,
    /// An error occurred while [setting a trap](TrapSet::set_action).
    #[error(transparent)]
    SetAction(#[from] SetActionError),
}

/// Information of an error that occurred while executing the `trap` built-in
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub struct Error {
    /// The cause of the error
    pub cause: ErrorCause,
    /// The condition for which the trap action could not be set
    pub cond: CondSpec,
    /// The field that specifies the condition
    pub field: Field,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl MessageBase for Error {
    fn message_title(&self) -> Cow<str> {
        "cannot update trap".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        Annotation::new(
            AnnotationType::Error,
            self.cause.to_string().into(),
            &self.field.origin,
        )
    }
}

/// Updates an action for a condition in the trap set.
///
/// This is a utility function for implementing [`Command::execute`].
fn set_action(
    traps: &mut TrapSet,
    system: &mut SharedSystem,
    cond: CondSpec,
    field: Field,
    action: Action,
    override_ignore: bool,
) -> Result<(), Error> {
    let Some(cond2) = cond.resolve(system) else {
        let cause = ErrorCause::UnsupportedSignal;
        return Err(Error { cause, cond, field });
    };
    traps
        .set_action(
            system,
            cond2,
            action.clone(),
            field.origin.clone(),
            override_ignore,
        )
        .map_err(|cause| {
            let cause = cause.into();
            Error { cause, cond, field }
        })
}

impl Command {
    /// Executes the trap built-in.
    ///
    /// If successful, returns a string that should be printed to the standard
    /// output. On failure, returns a non-empty list of errors.
    pub fn execute(self, env: &mut Env) -> Result<String, Vec<Error>> {
        match self {
            Self::PrintAll => Ok(display_traps(&env.traps, &env.system)),

            Self::SetAction { action, conditions } => {
                let override_ignore = env.options.get(Interactive) == On;

                let errors = conditions
                    .into_iter()
                    .filter_map(|(cond, field)| {
                        set_action(
                            &mut env.traps,
                            &mut env.system,
                            cond,
                            field,
                            action.clone(),
                            override_ignore,
                        )
                        .err()
                    })
                    .collect::<Vec<Error>>();

                if errors.is_empty() {
                    Ok(String::new())
                } else {
                    Err(errors)
                }
            }
        }
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
            errors.retain(|error| error.cause != SetActionError::InitiallyIgnored.into());

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
    use crate::Result;
    use futures_util::future::FutureExt;
    use std::ops::ControlFlow::{Break, Continue};
    use std::rc::Rc;
    use yash_env::io::Fd;
    use yash_env::semantics::Divert;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::system::r#virtual::{SIGINT, SIGPIPE, SIGUSR1, SIGUSR2};
    use yash_env::system::Disposition;
    use yash_env::Env;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;

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
        assert_eq!(process.disposition(SIGUSR1), Disposition::Ignore);
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
        assert_eq!(process.disposition(SIGUSR2), Disposition::Catch);
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
        assert_eq!(process.disposition(SIGPIPE), Disposition::Default);
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
        let args = Field::dummies(["echo", "EXIT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["echo t", "TERM"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "trap -- echo EXIT\ntrap -- 'echo t' TERM\n")
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
    fn initially_ignored_signal_not_modifiable_if_non_interactive() {
        let mut system = VirtualSystem::new();
        system
            .current_process_mut()
            .set_disposition(SIGINT, Disposition::Ignore);
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
            system.current_process().disposition(SIGINT),
            Disposition::Ignore
        );
    }

    #[test]
    fn modifying_initially_ignored_signal_in_interactive_mode() {
        let mut system = VirtualSystem::new();
        system
            .current_process_mut()
            .set_disposition(SIGINT, Disposition::Ignore);
        let mut env = Env::with_system(Box::new(system.clone()));
        env.options.set(Interactive, On);
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: Field::dummy("trap"),
            is_special: true,
        }));
        let args = Field::dummies(["echo", "INT"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stderr(&system.state, |stderr| assert_eq!(stderr, ""));
        assert_eq!(
            system.current_process().disposition(SIGINT),
            Disposition::Catch
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
