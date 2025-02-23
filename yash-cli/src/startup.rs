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

//! Shell startup

use self::args::{Run, Source, Work};
use yash_builtin::BUILTINS;
use yash_env::Env;
use yash_env::System;
use yash_env::io::Fd;
use yash_env::option::Option::{Interactive, Monitor, Stdin};
use yash_env::option::State::On;

pub mod args;
pub mod init_file;
pub mod input;

/// Tests whether the shell should be implicitly interactive.
///
/// As per POSIX, "if there are no operands and the shell's standard input and
/// standard error are attached to a terminal, the shell is considered to be
/// interactive." This function implements this rule.
pub fn auto_interactive<S: System>(system: &S, run: &Run) -> bool {
    if run.work.source != Source::Stdin {
        return false;
    }
    if run.options.iter().any(|&(o, _)| o == Interactive) {
        return false;
    }
    if !run.positional_params.is_empty() {
        return false;
    }
    system.isatty(Fd::STDIN) && system.isatty(Fd::STDERR)
}

/// Get the environment ready for performing the work.
///
/// This function takes the parsed command-line arguments and applies them to
/// the environment. It also sets up signal dispositions and prepares built-ins
/// and variables. The function returns the work to be performed, which is
/// extracted from the `run` argument.
///
/// This function is _pure_ in that all system calls are performed by the
/// `System` trait object (`env.system`).
pub fn configure_environment(env: &mut Env, run: Run) -> Work {
    // Apply the parsed options to the environment
    if auto_interactive(&env.system, &run) {
        env.options.set(Interactive, On);
    }
    if run.work.source == self::args::Source::Stdin {
        env.options.set(Stdin, On);
    }
    for &(option, state) in &run.options {
        env.options.set(option, state);
    }
    if env.options.get(Interactive) == On && !run.options.iter().any(|&(o, _)| o == Monitor) {
        env.options.set(Monitor, On);
    }

    // Apply the parsed operands to the environment
    env.arg0 = run.arg0;
    env.variables.positional_params_mut().values = run.positional_params;

    // Configure internal dispositions for signals
    if env.options.get(Interactive) == On {
        env.traps
            .enable_internal_dispositions_for_terminators(&mut env.system)
            .ok();
        if env.options.get(Monitor) == On {
            env.traps
                .enable_internal_dispositions_for_stoppers(&mut env.system)
                .ok();
        }
    }

    // Prepare built-ins
    env.builtins.extend(BUILTINS.iter().cloned());

    // Prepare variables
    env.init_variables();

    run.work
}
