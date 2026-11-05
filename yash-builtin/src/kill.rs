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
//! This module implements the [`kill` built-in], which sends a signal to processes.
//!
//! [`kill` built-in]: https://magicant.github.io/yash-rs/builtins/kill.html

use crate::common::help::{
    BuiltinHelp, HELP_OPTION, Help, WithHelp, print_help_of_current_builtin,
};
use crate::common::report::report_error;
use crate::common::syntax::OptionSpec;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::signal::RawNumber;
use yash_env::system::concurrency::WriteAll;
use yash_env::system::{Isatty, SendSignal, Signals};

/// Help information of the `kill` built-in
///
/// The built-in parses its arguments without [`OptionSpec`]s because of the
/// special syntax of the signal-specifying option, so the option list here is
/// used only for the help and must be kept in sync with the [`syntax`] module.
pub const HELP: &dyn Help = &BuiltinHelp {
    summary: "send a signal to processes",
    usage: &[
        "kill [-s SIGNAL|-n SIGNAL|-SIGNAL] TARGET...",
        "kill -l|-v [SIGNAL|EXIT_STATUS...]",
    ],
    options: &[
        OptionSpec::new()
            .short('s')
            .argument("SIGNAL")
            .description("send SIGNAL instead of TERM"),
        OptionSpec::new()
            .short('n')
            .argument("SIGNAL")
            .extension(true)
            .description("same as -s"),
        OptionSpec::new()
            .short('l')
            .description("list signal names"),
        OptionSpec::new()
            .short('v')
            .extension(true)
            .description("list signal numbers and names"),
        HELP_OPTION,
    ],
    manual_page_name: "kill",
};

/// Parsed command line arguments
///
/// # Design notes
///
/// `Command::Print::signals` contains raw operands received as command-line
/// arguments. Since determining whether a number represents a signal number or
/// an exit status falls within the semantics of [`mod@print`], these operands
/// remain unconverted to specific signals by the time arguments are parsed as
/// `Command::Print`. In contrast, `Command::Send` requires signal name
/// validation during command-line option parsing, so even when signals are
/// specified by name, `Command::Send::signal` already contains a signal
/// converted to its numerical value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Sends a signal to processes
    Send {
        /// Signal to send
        ///
        /// This signal number may represent an invalid signal if it was
        /// specified by number on the command line.
        signal: RawNumber,
        /// Parameter that specified the signal, if any
        signal_origin: Option<Field>,
        /// Target processes
        targets: Vec<Field>,
    },

    /// Lists signal names or descriptions
    Print {
        /// Signals to list
        ///
        /// Field values can be:
        ///
        /// - Signal names, e.g., `HUP`, `TERM`;
        /// - Signal numbers, e.g., `1`, `15`; or
        /// - Exit statuses representing signals, e.g., `386` for `SIGINT`.
        ///
        /// If empty, all signals are listed.
        signals: Vec<Field>,
        /// Whether to print descriptions
        verbose: bool,
    },
}

pub mod print;
pub mod send;
pub mod syntax;

impl Command {
    /// Executes the built-in.
    pub async fn execute<S>(&self, env: &mut Env<S>) -> crate::Result
    where
        S: Isatty + SendSignal + Signals + WriteAll,
    {
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
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> crate::Result
where
    S: Isatty + SendSignal + Signals + WriteAll,
{
    match syntax::parse(env, args) {
        Ok(WithHelp::Main(command)) => command.execute(env).await,
        Ok(WithHelp::Help) => print_help_of_current_builtin(env).await,
        Err(error) => report_error(env, &error).await,
    }
}
