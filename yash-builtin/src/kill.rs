// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Kill built-in
//!
//! The **`kill`** built-in sends a signal to processes.
//!
//! # Synopsis
//!
//! ```sh
//! kill [-s SIGNAL|-n SIGNAL|-SIGNAL] target…
//! ```
//!
//! ```sh
//! kill -l|-v [SIGNAL|exit_status]…
//! ```
//!
//! # Description
//!
//! Without the `-l` or `-v` option, the built-in sends a signal to processes.
//!
//! With the `-l` or `-v` option, the built-in lists signal names or
//! descriptions.
//!
//! # Options
//!
//! The **`-s`** or **`-n`** option specifies the signal to send. The signal
//! name is case-insensitive, but must be specified without the `SIG` prefix.
//! The default signal is `SIGTERM`. (TODO: Allow the `SIG` prefix)
//!
//! The signal may be specified as a number instead of a name. If the number
//! is zero, the built-in does not send a signal, but instead checks whether
//! the shell can send the signal to the target processes.
//!
//! The obsolete syntax allows the signal name or number to be specified
//! directly after the hyphen like `-TERM` and `-15` instead of `-s TERM` and
//! `-n 15`.
//!
//! The **`-l`** option lists signal names. The names are printed one per line,
//! without the `SIG` prefix.
//!
//! The **`-v`** option lists signal descriptions. This works like the `-l`
//! option, but prints the signal number, name, and description instead of
//! just the name. The `-v` option may be used with the `-l` option, in which
//! case the `-l` option is ignored. (TODO: The description is not yet printed)
//!
//! # Operands
//!
//! Without the `-l` or `-v` option, the built-in takes one or more operands
//! that specify the target processes. Each operand is one of the following:
//!
//! - A positive decimal integer, which should be a process ID
//! - A negative decimal integer, which should be a negated process group ID
//! - `0`, which means the current process group
//! - `-1`, which means all processes
//! - A [job ID](yash_env::job::id) with a leading `%`
//!
//! With the `-l` or `-v` option, the built-in may take operands that limit the
//! output to the specified signals. Each operand is one of the following:
//!
//! - The exit status of a process that was terminated by a signal
//! - A signal number
//! - A signal name without the `SIG` prefix
//!
//! Without operands, the `-l` and `-v` options list all signals.
//!
//! # Errors
//!
//! It is an error if:
//!
//! - The `-l` or `-v` option is not specified and no target processes are
//!   specified.
//! - A specified signal is not supported by the shell.
//! - A specified target process does not exist.
//! - The target job specified by a job ID operand is not [job-controlled] by
//!   the shell.
//! - The signal cannot be sent to any of the target processes specified by an
//!   operand.
//! - An operand specified with the `-l` or `-v` option does not identify a
//!   supported signal.
//!
//! [job-controlled]: yash_env::job::Job::job_controlled
//!
//! # Exit status
//!
//! The exit status is zero unless an error occurs. The exit status is zero if
//! the signal is sent to at least one process for each operand, even if the
//! signal cannot be sent to some of the processes.
//!
//! # Usage notes
//!
//! When a target is specified as a job ID, the built-in cannot tell whether
//! the job process group still exists. If the job process group has been
//! terminated and another process group has been created with the same
//! process group ID, the built-in will send the signal to the new process
//! group.
//!
//! # Portability
//!
//! Specifying a signal number other than `0` to the `-s` option is a
//! non-standard extension.
//!
//! Specifying a signal number to the `-n` option is a ksh extension. This
//! implementation also supports the `-n` option with a signal name.
//!
//! The `kill -SIGNAL target…` form may not be parsed as expected by other
//! implementations when the signal name starts with an `s`. For example, `kill
//! -stop 123` may try to send the `SIGTOP` signal instead of the `SIGSTOP`
//! signal.
//!
//! POSIX defines the following signal numbers:
//!
//! - `0` (a dummy signal that can be used to check whether the shell can send
//!   a signal to a process)
//! - `1` (`SIGHUP`)
//! - `2` (`SIGINT`)
//! - `3` (`SIGQUIT`)
//! - `6` (`SIGABRT`)
//! - `9` (`SIGKILL`)
//! - `14` (`SIGALRM`)
//! - `15` (`SIGTERM`)
//!
//! Other signal numbers are implementation-defined.
//!
//! Using the `-l` option with more than one operand is a non-standard
//! extension. Specifying a signal name operand to the `-l` option is a
//! non-standard extension.
//!
//! The `-v` option is a non-standard extension.
//!
//! Some implementations print `0` or `EXIT` for `kill -l 0` or `kill -l EXIT`
//! while this implementation regards them as invalid operands.
//!
//! On some systems, a signal may have more than one name. There seems to be no
//! consensus whether `kill -l` should print all names or just one name for each
//! signal. This implementation currently prints all names, but this behavior
//! may change in the future.

use crate::common::report_error;
use yash_env::Env;
use yash_env::semantics::Field;

mod signal;

pub use signal::Signal;

/// Parsed command line arguments
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Sends a signal to processes
    Send {
        /// Signal to send
        signal: Signal,
        /// Parameter that specified the signal, if any
        signal_origin: Option<Field>,
        /// Target processes
        targets: Vec<Field>,
    },
    /// Lists signal names or descriptions
    Print {
        /// Signals to list
        ///
        /// If empty, all signals are listed.
        signals: Vec<(Signal, Field)>,
        /// Whether to print descriptions
        verbose: bool,
    },
}

pub mod print;
pub mod send;
pub mod syntax;

impl Command {
    /// Executes the built-in.
    pub async fn execute(&self, env: &mut Env) -> crate::Result {
        match self {
            Self::Send {
                signal,
                signal_origin,
                targets,
            } => send::execute(env, *signal, signal_origin.as_ref(), targets).await,

            Self::Print { signals, verbose } => print::execute(env, signals, *verbose).await,
        }
    }
}

/// Entry point of the kill built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    match syntax::parse(env, args) {
        Ok(command) => command.execute(env).await,
        Err(error) => report_error(env, error.to_message()).await,
    }
}
