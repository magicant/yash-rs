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

//! Type definitions for command execution.

use crate::Env;
use crate::signal;
use crate::system::System;
use std::cell::RefCell;
use std::ffi::c_int;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::process::ExitCode;
use std::process::Termination;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Location;

/// Resultant string of word expansion.
///
/// A field is a string accompanied with the original word location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    /// String value of the field.
    pub value: String,
    /// Location of the word this field resulted from.
    pub origin: Location,
}

impl Field {
    /// Creates a new field with a dummy origin location.
    ///
    /// The value of the resulting field will be `value.into()`.
    /// The origin of the field will be created by [`Location::dummy`] with a
    /// clone of the value.
    #[inline]
    pub fn dummy<S: Into<String>>(value: S) -> Field {
        fn with_value(value: String) -> Field {
            let origin = Location::dummy(value.clone());
            Field { value, origin }
        }
        with_value(value.into())
    }

    /// Creates an array of fields with dummy origin locations.
    ///
    /// This function calls [`dummy`](Self::dummy) to create the results.
    pub fn dummies<I, S>(values: I) -> Vec<Field>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        values.into_iter().map(Self::dummy).collect()
    }
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

/// Number that summarizes the result of command execution.
///
/// An exit status is an integer returned from a utility (or command) when
/// executed. It usually is a summarized result of the execution.  Many
/// utilities return an exit status of zero when successful and non-zero
/// otherwise.
///
/// In the shell language, the special parameter `$?` expands to the exit status
/// of the last executed command. Exit statuses also affect the behavior of some
/// compound commands.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExitStatus(pub c_int);

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<c_int> for ExitStatus {
    fn from(value: c_int) -> ExitStatus {
        ExitStatus(value)
    }
}

impl From<ExitStatus> for c_int {
    fn from(exit_status: ExitStatus) -> c_int {
        exit_status.0
    }
}

/// Converts a signal number to the corresponding exit status.
///
/// POSIX requires the exit status to be greater than 128. The current
/// implementation returns `signal_number + 384`.
///
/// See [`ExitStatus::to_signal`] for the reverse conversion.
impl From<signal::Number> for ExitStatus {
    fn from(number: signal::Number) -> Self {
        Self::from(number.as_raw() + 0x180)
    }
}

impl ExitStatus {
    /// Returns the signal name and number corresponding to the exit status.
    ///
    /// This function is the inverse of the `From<signal::Number>` implementation
    /// for `ExitStatus`. It tries to find a signal name and number by offsetting
    /// the exit status by 384. If the offsetting does not result in a valid signal
    /// name and number, it additionally tries with 128 and 0 unless `exact` is
    /// `true`.
    ///
    /// If `self` is not a valid signal exit status, this function returns `None`.
    #[must_use]
    pub fn to_signal<S: System + ?Sized>(
        self,
        system: &S,
        exact: bool,
    ) -> Option<(signal::Name, signal::Number)> {
        fn convert<S: System + ?Sized>(
            exit_status: ExitStatus,
            offset: c_int,
            system: &S,
        ) -> Option<(signal::Name, signal::Number)> {
            let number = exit_status.0.checked_sub(offset)?;
            system.validate_signal(number)
        }

        if let Some(signal) = convert(self, 0x180, system) {
            return Some(signal);
        }
        if exact {
            return None;
        }
        if let Some(signal) = convert(self, 0x80, system) {
            return Some(signal);
        }
        if let Some(signal) = convert(self, 0, system) {
            return Some(signal);
        }
        None
    }
}

/// Converts the exit status to `ExitCode`.
///
/// Note that `ExitCode` only supports exit statuses in the range of 0 to 255.
/// Only the lowest 8 bits of the exit status are used in the conversion.
impl Termination for ExitStatus {
    fn report(self) -> ExitCode {
        (self.0 as u8).into()
    }
}

impl ExitStatus {
    /// Exit status of 0: success
    pub const SUCCESS: ExitStatus = ExitStatus(0);

    /// Exit status of 1: failure
    pub const FAILURE: ExitStatus = ExitStatus(1);

    /// Exit status of 2: error severer than failure
    pub const ERROR: ExitStatus = ExitStatus(2);

    /// Exit Status of 126: command not executable
    pub const NOEXEC: ExitStatus = ExitStatus(126);

    /// Exit status of 127: command not found
    pub const NOT_FOUND: ExitStatus = ExitStatus(127);

    /// Exit status of 128: unrecoverable read error
    pub const READ_ERROR: ExitStatus = ExitStatus(128);

    /// Returns true if and only if `self` is zero.
    pub const fn is_successful(&self) -> bool {
        self.0 == 0
    }
}

/// Result of interrupted command execution.
///
/// `Divert` implements `Ord`. Values are ordered by severity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Divert {
    /// Continue the current loop.
    Continue {
        /// Number of loops to break before continuing.
        ///
        /// `0` for continuing the innermost loop, `1` for one-level outer, and so on.
        count: usize,
    },

    /// Break the current loop.
    Break {
        /// Number of loops to break.
        ///
        /// `0` for breaking the innermost loop, `1` for one-level outer, and so on.
        count: usize,
    },

    /// Return from the current function or script.
    Return(Option<ExitStatus>),

    /// Interrupt the current shell execution environment.
    ///
    /// This is the same as `Exit` in a non-interactive shell: it makes the
    /// shell exit after executing the EXIT trap, if any. If this is used inside
    /// the EXIT trap, the shell will exit immediately.
    ///
    /// In an interactive shell, this will abort the currently executed command
    /// and resume prompting for a next command line.
    Interrupt(Option<ExitStatus>),

    /// Exit from the current shell execution environment.
    ///
    /// This makes the shell exit after executing the EXIT trap, if any.
    /// If this is used inside the EXIT trap, the shell will exit immediately.
    Exit(Option<ExitStatus>),

    /// Exit from the current shell execution environment immediately.
    ///
    /// This makes the shell exit without executing the EXIT trap.
    Abort(Option<ExitStatus>),
}

impl Divert {
    /// Returns the exit status associated with the `Divert`.
    ///
    /// Returns the variant's value if `self` is `Exit` or `Interrupt`;
    /// otherwise, `None`.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        use Divert::*;
        match self {
            Continue { .. } | Break { .. } => None,
            Return(exit_status)
            | Interrupt(exit_status)
            | Exit(exit_status)
            | Abort(exit_status) => *exit_status,
        }
    }
}

/// Result of command execution.
///
/// If the command was interrupted in the middle of execution, the result value
/// will be a `Break` having a [`Divert`] value which specifies what to execute
/// next.
pub type Result<T = ()> = ControlFlow<Divert, T>;

/// Wrapper for running a read-eval-loop
///
/// This struct declares a function type for running a read-eval-loop. An
/// implementation of this function type should be provided and stored in the
/// environment's [`any`](Env::any) storage so that it can be used by modules
/// depending on read-eval-loop execution.
///
/// The function takes two arguments. The first argument is a mutable reference
/// to an environment wrapped in a [`RefCell`]. The second argument is a mutable
/// reference to a [`Lexer`] from which commands are read. The `RefCell` can be
/// shared with the [`Input`](crate::input::Input) implementor used by the
/// lexer, so that the implementor can access and modify the environment while
/// reading commands. After reading a command, the `RefCell` should be borrowed
/// mutably outside the lexer to execute the command. The `RefCell` should not
/// be borrowed otherwise during the read-eval-loop execution.
///
/// The function returns a future which resolves to a [`Result`] when awaited.
/// The function should execute commands read from the lexer until the end of
/// input or a [`Divert`] is encountered.
///
/// The function should set [`Env::exit_status`] appropriately after the loop
/// ends. If the input contains no commands, the exit status should be set to
/// `ExitStatus(0)`.
///
/// The function should also
/// [update subshell statuses](Env::update_all_subshell_statuses) and handle
/// traps during the loop execution as specified in the shell semantics.
///
/// The most standard implementation of this function type is provided in the
/// [`yash-semantics` crate](https://crates.io/crates/yash-semantics):
///
/// ```
/// # use yash_env::semantics::RunReadEvalLoop;
/// let mut env = yash_env::Env::new_virtual();
/// env.any.insert(Box::new(RunReadEvalLoop(|env, lexer| {
///     Box::pin(async move { yash_semantics::read_eval_loop(env, lexer).await })
/// })));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RunReadEvalLoop(
    #[allow(clippy::type_complexity)]
    pub  for<'a> fn(
        &'a RefCell<&mut Env>,
        &'a mut Lexer,
    ) -> Pin<Box<dyn Future<Output = Result> + 'a>>,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::r#virtual::VirtualSystem;
    use crate::system::r#virtual::{SIGINT, SIGTERM};

    #[test]
    fn exit_status_to_signal() {
        let system = VirtualSystem::new();

        assert_eq!(ExitStatus(0).to_signal(&system, false), None);
        assert_eq!(ExitStatus(0).to_signal(&system, true), None);

        assert_eq!(
            ExitStatus(SIGINT.as_raw()).to_signal(&system, false),
            Some((signal::Name::Int, SIGINT))
        );
        assert_eq!(ExitStatus(SIGINT.as_raw()).to_signal(&system, true), None);

        assert_eq!(
            ExitStatus(SIGINT.as_raw() + 0x80).to_signal(&system, false),
            Some((signal::Name::Int, SIGINT))
        );
        assert_eq!(
            ExitStatus(SIGINT.as_raw() + 0x80).to_signal(&system, true),
            None
        );

        assert_eq!(
            ExitStatus(SIGINT.as_raw() + 0x180).to_signal(&system, false),
            Some((signal::Name::Int, SIGINT))
        );
        assert_eq!(
            ExitStatus(SIGINT.as_raw() + 0x180).to_signal(&system, true),
            Some((signal::Name::Int, SIGINT))
        );

        assert_eq!(
            ExitStatus(SIGTERM.as_raw() + 0x180).to_signal(&system, false),
            Some((signal::Name::Term, SIGTERM))
        );
        assert_eq!(
            ExitStatus(SIGTERM.as_raw() + 0x180).to_signal(&system, true),
            Some((signal::Name::Term, SIGTERM))
        );
    }
}
