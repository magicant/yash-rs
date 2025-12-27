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

//! This is an internal library crate for the yash shell. Currently, **this
//! crate is not intended to be used as a library by other crates. No part of
//! this crate is covered by semantic versioning.**
//!
//! The entry point for the shell is the [`main`] function, which is to be used
//! as the `main` function in the binary crate. The function sets up the shell
//! environment and runs the main read-eval loop.

pub mod startup;
// mod runner;

use self::startup::args::Parse;
use self::startup::init_file::run_rcfile;
use self::startup::input::prepare_input;
use std::cell::RefCell;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::Env;
use yash_env::RealSystem;
use yash_env::System;
use yash_env::option::{Interactive, On};
use yash_env::signal;
use yash_env::system::Signals as _;
use yash_env::system::SystemEx as _;
use yash_env::system::{Disposition, Errno};
use yash_executor::Executor;
use yash_semantics::trap::run_exit_trap;
use yash_semantics::{Divert, ExitStatus};
use yash_semantics::{interactive_read_eval_loop, read_eval_loop};

async fn print_version<S: System>(env: &mut Env<S>) {
    let version = env!("CARGO_PKG_VERSION");
    let result = yash_builtin::common::output(env, &format!("yash {version}\n")).await;
    env.exit_status = result.exit_status();
}

// The RefCell is local to this function, so it is safe to keep borrows across await points.
#[allow(clippy::await_holding_refcell_ref)]
async fn run_as_shell_process<S: System + 'static>(env: &mut Env<S>) {
    // Parse the command-line arguments
    let run = match self::startup::args::parse(std::env::args()) {
        Ok(Parse::Help) => todo!("print help"),
        Ok(Parse::Version) => return print_version(env).await,
        Ok(Parse::Run(run)) => run,
        Err(e) => {
            let arg0 = std::env::args().next().unwrap_or_else(|| "yash".to_owned());
            env.system.print_error(&format!("{arg0}: {e}\n")).await;
            env.exit_status = ExitStatus::ERROR;
            return;
        }
    };

    // Import environment variables
    env.variables.extend_env(std::env::vars());

    let work = self::startup::configure_environment(env, run).await;

    let is_interactive = env.options.get(Interactive) == On;

    // Run initialization files
    // TODO run profile if login
    run_rcfile(env, work.rcfile).await;

    // Prepare the input for the main read-eval loop
    let ref_env = RefCell::new(env);
    let lexer = match prepare_input(&ref_env, &work.source) {
        Ok(lexer) => lexer,
        Err(e) => {
            let arg0 = std::env::args().next().unwrap_or_else(|| "yash".to_owned());
            let message = format!("{arg0}: {e}\n");
            // The borrow checker of Rust 1.79.0 is not smart enough to reason
            // about the lifetime of `e` here, so we re-borrow from `ref_env`
            // instead of taking `env` out of `ref_env`.
            // let mut env = ref_env.into_inner();
            let mut env = ref_env.borrow_mut();
            env.system.print_error(&message).await;
            env.exit_status = match e.errno {
                Errno::ENOENT | Errno::ENOTDIR | Errno::EILSEQ => ExitStatus::NOT_FOUND,
                _ => ExitStatus::NOEXEC,
            };
            return;
        }
    };

    // Run the read-eval loop
    let result = if is_interactive {
        interactive_read_eval_loop(&ref_env, &mut { lexer }).await
    } else {
        read_eval_loop(&ref_env, &mut { lexer }).await
    };

    let env = ref_env.into_inner();
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
}

pub fn main() -> ! {
    // SAFETY: This is the only instance of RealSystem we create in the whole
    // process.
    let system = unsafe { RealSystem::new() };
    let mut env = Env::with_system(system);

    // Rust by default sets SIGPIPE to SIG_IGN, which is not desired.
    // As an imperfect workaround, we set SIGPIPE to SIG_DFL here.
    // TODO Use unix_sigpipe: https://github.com/rust-lang/rust/issues/97889
    let sigpipe = env
        .system
        .signal_number_from_name(signal::Name::Pipe)
        .unwrap();
    _ = env.system.sigaction(sigpipe, Disposition::Default);

    let system = env.system.clone();
    let executor = Executor::new();
    let task = Box::pin(async {
        run_as_shell_process(&mut env).await;
        env.system.exit_or_raise(env.exit_status).await;
    });
    // SAFETY: We never create new threads in the whole process, so wakers are
    // never shared between threads.
    unsafe { executor.spawn_pinned(task) }
    loop {
        executor.run_until_stalled();
        system.select(false).ok();
    }
}
