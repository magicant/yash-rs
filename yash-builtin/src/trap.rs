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
//! This module implements the [`trap` built-in], which sets or prints traps.
//!
//! [`trap` built-in]: https://magicant.github.io/yash-rs/builtins/trap.html
//!
//! # Implementation notes
//!
//! The [`TrapSet`] remembers the traps that were configured in the parent shell
//! so that the built-in can print them when invoked in a subshell. Those traps
//! are cleared when the built-in modifies any trap in the subshell. See
//! [`TrapSet::enter_subshell`] and [`TrapSet::set_action`] for details.

mod cond;

pub use self::cond::CondSpec;
use crate::common::report::merge_reports;
use crate::common::report::report_error;
use crate::common::report::report_failure;
use crate::common::syntax::Mode;
use crate::common::syntax::parse_arguments;
use itertools::Itertools as _;
use thiserror::Error;
use yash_env::Env;
use yash_env::option::Option::Interactive;
use yash_env::option::State::On;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Report, ReportType, Snippet};
use yash_env::system::{Fcntl, Isatty, SharedSystem, Sigaction, Sigmask, Signals, Write};
use yash_env::trap::Action;
use yash_env::trap::Condition;
use yash_env::trap::SetActionError;
use yash_env::trap::SignalSystem;
use yash_env::trap::TrapSet;
use yash_quote::quoted;

/// Interpretation of command line arguments that selects the behavior of the
/// `trap` built-in
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Print all traps
    PrintAll {
        /// If true, print all traps including ones with the default action
        include_default: bool,
    },

    /// Print traps for one or more conditions
    Print {
        /// The conditions for which to print traps
        conditions: Vec<(CondSpec, Field)>,
    },

    /// Set an action for one or more conditions
    SetAction {
        /// The action to set
        action: Action,
        /// The conditions for which the action should be set
        conditions: Vec<(CondSpec, Field)>,
    },
}

pub mod syntax;

/// Displays the current trap for a condition.
///
/// If the trap is not set and `include_default` is `false`, this function
/// does nothing. Otherwise, it prints the trap in the format `trap -- command
/// condition`. The result is written to `output`.
fn display_trap<S: SignalSystem, W: std::fmt::Write>(
    traps: &mut TrapSet,
    system: &S,
    cond: Condition,
    include_default: bool,
    output: &mut W,
) -> Result<(), std::fmt::Error> {
    let Ok(trap) = traps.peek_state(system, cond) else {
        return Ok(());
    };
    let command = match &trap.action {
        Action::Default if include_default => "-",
        Action::Default => return Ok(()),
        Action::Ignore => "",
        Action::Command(command) => command,
    };
    let cond = cond.to_string(system);
    writeln!(output, "trap -- {} {}", quoted(command), cond)
}

/// Returns a string that represents the currently configured traps.
///
/// The returned string is the whole output of the `trap` built-in
/// used without options or operands, including the trailing newline.
///
/// This function is equivalent to [`display_all_traps`] with `include_default`
/// set to `false`.
#[must_use]
pub fn display_traps<S: SignalSystem>(traps: &mut TrapSet, system: &S) -> String {
    display_all_traps(traps, system, false)
}

/// Returns a string that represents the currently configured traps.
///
/// The returned string is the whole output of the `trap` built-in
/// used without operands, including the trailing newline.
///
/// If `include_default` is `true`, the output includes traps with the default
/// action. Otherwise, the output includes only traps with non-default actions.
#[must_use]
pub fn display_all_traps<S: SignalSystem>(
    traps: &mut TrapSet,
    system: &S,
    include_default: bool,
) -> String {
    let mut output = String::new();
    for cond in Condition::iter(system) {
        if let Condition::Signal(number) = cond {
            if number == S::SIGKILL || number == S::SIGSTOP {
                continue;
            }
        }
        display_trap(traps, system, cond, include_default, &mut output).unwrap()
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
    /// The condition on which the error occurred
    pub cond: CondSpec,
    /// The field that specifies the condition
    pub field: Field,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}

impl Error {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = match &self.cause {
            ErrorCause::UnsupportedSignal => "invalid trap condition".into(),
            ErrorCause::SetAction(_) => "cannot update trap".into(),
        };
        report.snippets =
            Snippet::with_primary_span(&self.field.origin, self.cause.to_string().into());
        report
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

/// Resolves a condition specification to a condition.
fn resolve<S: Signals>(cond: CondSpec, field: Field, system: &S) -> Result<Condition, Error> {
    cond.to_condition(system).ok_or_else(|| {
        let cause = ErrorCause::UnsupportedSignal;
        Error { cause, cond, field }
    })
}

/// Updates an action for a condition in the trap set.
///
/// This is a utility function for implementing [`Command::execute`].
fn set_action<S>(
    traps: &mut TrapSet,
    system: &mut SharedSystem<S>,
    cond: CondSpec,
    field: Field,
    action: Action,
    override_ignore: bool,
) -> Result<(), Error>
where
    S: Signals + Sigmask + Sigaction,
{
    let Some(cond2) = cond.to_condition(system) else {
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
    pub fn execute<S>(self, env: &mut Env<S>) -> Result<String, Vec<Error>>
    where
        S: Signals + Sigmask + Sigaction,
    {
        match self {
            Self::PrintAll { include_default } => Ok(display_all_traps(
                &mut env.traps,
                &env.system,
                include_default,
            )),

            Self::Print { conditions } => {
                let mut output = String::new();

                let ((), errors): ((), Vec<Error>) = conditions
                    .into_iter()
                    .map(|(cond, field)| {
                        let cond = resolve(cond, field, &env.system)?;
                        display_trap(&mut env.traps, &env.system, cond, true, &mut output).unwrap();
                        Ok(())
                    })
                    .partition_result();

                if errors.is_empty() {
                    Ok(output)
                } else {
                    Err(errors)
                }
            }

            Self::SetAction { action, conditions } => {
                let override_ignore = env.options.get(Interactive) == On;

                let ((), errors): ((), Vec<Error>) = conditions
                    .into_iter()
                    .map(|(cond, field)| {
                        set_action(
                            &mut env.traps,
                            &mut env.system,
                            cond,
                            field,
                            action.clone(),
                            override_ignore,
                        )
                    })
                    .partition_result();

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
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: Fcntl + Isatty + Signals + Sigmask + Sigaction + Write,
{
    let (options, operands) = match parse_arguments(syntax::OPTION_SPECS, Mode::with_env(env), args)
    {
        Ok(result) => result,
        Err(error) => return report_error(env, &error).await,
    };

    let command = match syntax::interpret(options, operands) {
        Ok(command) => command,
        Err(errors) => {
            let is_soft_failure = errors
                .iter()
                .all(|e| matches!(e, syntax::Error::UnknownCondition(_)));
            let report = merge_reports(&errors).unwrap();
            let mut result = report_error(env, report).await;
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

            match merge_reports(&errors) {
                None => crate::Result::default(),
                Some(report) => report_failure(env, report).await,
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
    use yash_env::Env;
    use yash_env::VirtualSystem;
    use yash_env::io::Fd;
    use yash_env::semantics::Divert;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::system::Disposition;
    use yash_env::system::r#virtual::{SIGINT, SIGPIPE, SIGUSR1, SIGUSR2};
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;

    #[test]
    fn setting_trap_to_ignore() {
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn printing_some_trap() {
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
    fn printing_initially_ignored_trap() {
        let system = VirtualSystem::new();
        system
            .current_process_mut()
            .set_disposition(SIGINT, Disposition::Ignore);
        let mut env = Env::with_system(system.clone());

        let result = main(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&system.state, |stdout| {
            assert_eq!(stdout, "trap -- '' INT\n")
        });
    }

    #[test]
    fn printing_specified_traps() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["echo", "EXIT"]);
        let _ = main(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["echo t", "TERM"]);
        let _ = main(&mut env, args).now_or_never().unwrap();

        let result = main(&mut env, Field::dummies(["-p", "TERM", "INT"]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "trap -- 'echo t' TERM\ntrap -- - INT\n")
        });
    }

    #[test]
    fn error_printing_traps() {
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
        system
            .current_process_mut()
            .set_disposition(SIGINT, Disposition::Ignore);
        let mut env = Env::with_system(system.clone());
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
        let system = VirtualSystem::new();
        system
            .current_process_mut()
            .set_disposition(SIGINT, Disposition::Ignore);
        let mut env = Env::with_system(system.clone());
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
        let system = VirtualSystem::new();
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
