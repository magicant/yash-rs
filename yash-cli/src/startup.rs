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
use yash_env::prompt::GetPrompt;
use yash_env::trap::RunSignalTrapIfCaught;

pub mod args;
pub mod init_file;
pub mod input;

/// Tests whether the shell should be implicitly interactive.
///
/// As per POSIX, "if the shell reads commands from the standard input and the
/// shell's standard input and standard error are attached to a terminal, the
/// shell is considered to be interactive." This function implements this rule.
///
/// This function returns `false` if the interactive option is explicitly
/// specified in the command line arguments to honor the user's intent.
pub fn auto_interactive<S: System>(system: &S, run: &Run) -> bool {
    if run.work.source != Source::Stdin {
        return false;
    }
    if run.options.iter().any(|&(o, _)| o == Interactive) {
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

    // Make sure the shell is in the foreground if job control is enabled
    if env.options.get(Monitor) == On {
        // Ignore failures as we can still proceed even if we can't get into the foreground
        env.ensure_foreground().ok();
    }

    // Prepare built-ins
    env.builtins.extend(BUILTINS.iter().cloned());

    // Prepare variables
    env.init_variables();

    // Inject dependencies
    inject_dependencies(env);

    run.work
}

/// Inject dependencies into the environment.
fn inject_dependencies(env: &mut Env) {
    env.any.insert(Box::new(GetPrompt(|env, context| {
        Box::pin(async move {
            let prompt = yash_prompt::fetch_posix(&env.variables, context);
            yash_prompt::expand_posix(env, &prompt, false).await
        })
    })));

    env.any
        .insert(Box::new(RunSignalTrapIfCaught(|env, signal| {
            Box::pin(async move { yash_semantics::trap::run_trap_if_caught(env, signal).await })
        })));
}
