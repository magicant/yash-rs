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

//! This is a library crate that implements the command-line frontend for the
//! yash shell. It is used by the `yash3` binary crate to provide the shell
//! functionality. Currently, this crate is not intended to be used as a library
//! by other crates.
//!
//! The main entry point for the shell is the [`bin_main`] function, which is
//! called by the `main` function in the binary crate. The `bin_main` function
//! sets up the shell environment and runs the main read-eval loop.

pub mod startup;
// mod runner;

use futures_util::task::LocalSpawnExt as _;
use futures_util::FutureExt as _;
use startup::args::Parse;
use startup::prepare_input;
use std::cell::RefCell;
use std::num::NonZeroU64;
use std::ops::ControlFlow::{Break, Continue};
use yash_builtin::BUILTINS;
use yash_env::option::Option::{Interactive, Monitor, Stdin};
use yash_env::option::State::On;
use yash_env::signal;
use yash_env::system::SignalHandling;
use yash_env::Env;
use yash_env::RealSystem;
use yash_env::System;
use yash_semantics::trap::run_exit_trap;
use yash_semantics::ExitStatus;
use yash_semantics::{read_eval_loop, Divert};
use yash_syntax::parser::lex::Lexer;

async fn print_version(env: &mut Env) -> i32 {
    let version = env!("CARGO_PKG_VERSION");
    let result = yash_builtin::common::output(env, &format!("yash {}\n", version)).await;
    result.exit_status().0
}

// The RefCell is local to this function, so it is safe to keep borrows across await points.
#[allow(clippy::await_holding_refcell_ref)]
async fn parse_and_print(mut env: Env) -> i32 {
    // Parse the command-line arguments
    let run = match startup::args::parse(std::env::args()) {
        Ok(Parse::Help) => todo!("print help"),
        Ok(Parse::Version) => return print_version(&mut env).await,
        Ok(Parse::Run(run)) => run,
        Err(e) => {
            let arg0 = std::env::args().next().unwrap_or_else(|| "yash".to_owned());
            env.system.print_error(&format!("{}: {}\n", arg0, e)).await;
            return ExitStatus::ERROR.0;
        }
    };

    // Apply the parsed options to the environment
    if startup::auto_interactive(&env.system, &run) {
        env.options.set(Interactive, On);
    }
    if run.source == startup::args::Source::Stdin {
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

    // Initialize signal handlers
    if env.options.get(Interactive) == On {
        env.traps.enable_terminator_handlers(&mut env.system).ok();
        if env.options.get(Monitor) == On {
            env.traps.enable_stopper_handlers(&mut env.system).ok();
        }
    }

    // Prepare built-ins
    env.builtins.extend(BUILTINS.iter().cloned());

    // Prepare variables
    env.variables.extend_env(std::env::vars());
    env.init_variables();

    // TODO run profile if login
    // TODO run rcfile if interactive

    // Prepare the input for the main read-eval loop
    let ref_env = &RefCell::new(&mut env);
    let input = match prepare_input(ref_env, &run.source) {
        Ok(input) => input,
        Err(e) => {
            let arg0 = std::env::args().next().unwrap_or_else(|| "yash".to_owned());
            let message = format!("{}: {}\n", arg0, e);
            // The borrow checker of Rust 1.79.0 is not smart enough to reason
            // about the lifetime of `input` here, so we re-borrow from `ref_env`
            // instead of reusing `env`.
            // env.system.print_error(&message).await;
            ref_env.borrow_mut().system.print_error(&message).await;
            return ExitStatus::FAILURE.0;
        }
    };
    let line = NonZeroU64::new(1).unwrap();
    let mut lexer = Lexer::new(input.input, line, input.source);

    // Run the read-eval loop
    let result = read_eval_loop(ref_env, &mut lexer).await;

    // The borrow checker of Rust 1.79.0 is not smart enough to reason about the
    // lifetime of `input` here, so we re-borrow from `ref_env` instead of reusing `env`.
    // env.system.print_error(&message).await;
    let env = &mut **ref_env.borrow_mut();
    env.apply_result(result);

    match result {
        Continue(())
        | Break(Divert::Continue { .. })
        | Break(Divert::Break { .. })
        | Break(Divert::Return(_))
        | Break(Divert::Interrupt(_))
        | Break(Divert::Exit(_)) => run_exit_trap(env).await,
        Break(Divert::Abort(_)) => (),
    }

    env.exit_status.0
}

pub fn bin_main() -> i32 {
    // SAFETY: This is the only instance of RealSystem we create in the whole
    // process.
    let system = unsafe { RealSystem::new() };
    let mut env = Env::with_system(Box::new(system));

    // Rust by default sets SIGPIPE to SIG_IGN, which is not desired.
    // As an imperfect workaround, we set SIGPIPE to SIG_DFL here.
    // TODO Use unix_sigpipe: https://github.com/rust-lang/rust/issues/97889
    let sigpipe = env
        .system
        .signal_number_from_name(signal::Name::Pipe)
        .unwrap();
    _ = env.system.sigaction(sigpipe, SignalHandling::Default);

    let system = env.system.clone();
    let mut pool = futures_executor::LocalPool::new();
    let task = parse_and_print(env);
    let mut task = pool.spawner().spawn_local_with_handle(task).unwrap();
    loop {
        pool.run_until_stalled();
        if let Some(exit_status) = (&mut task).now_or_never() {
            return exit_status;
        }
        system.select(false).ok();
    }
}
