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
use std::fmt::Write as _;
use std::ops::ControlFlow::{Break, Continue};
use std::rc::Rc;
use yash_env::Env;
use yash_env::RealSystem;
use yash_env::option::{Interactive, Off, On, Portable};
use yash_env::semantics::{Divert, ExitStatus, exit_or_raise};
use yash_env::system::concurrency::WriteAll;
use yash_env::system::resource::GetRlimit;
use yash_env::system::{
    Chdir, Concurrent, Disposition, Errno, GetCwd, GetUid, Isatty, Sigaction as _, Signals as _,
    Sysconf, TcGetPgrp, Times, Umask, Write,
};
use yash_semantics::trap::run_exit_trap;
use yash_semantics::{Runtime, interactive_read_eval_loop, read_eval_loop};

async fn print_help<S>(env: &mut Env<S>)
where
    S: Isatty + WriteAll,
{
    let mut help = "\
Usage:
  yash3 [OPTION...] [FILE [ARGUMENT...]]
  yash3 [OPTION...] -c COMMAND [COMMAND_NAME [ARGUMENT...]]
  yash3 [OPTION...] -s [ARGUMENT...]

Startup options:
      --help
  -V, --version
      --profile=FILE
      --noprofile
      --rcfile=FILE
      --norcfile

Shell options:
  Turn on an option with -o NAME or --NAME, and turn it off with +o NAME or
  ++NAME. Prefixing NAME with \"no\" reverses the effect, as in -o noglob.
  Each single-letter option below does the same as the -o form beside it,
  and swapping its - or + reverses the effect.

"
    .to_owned();
    for option in yash_env::option::Option::iter() {
        match option.short_name() {
            Some((name, On)) => writeln!(help, "  -{name}  -o {option}"),
            Some((name, Off)) => writeln!(help, "  +{name}  -o {option}"),
            None => writeln!(help, "      -o {option}"),
        }
        .unwrap();
    }
    help.push_str(concat!(
        "\nSee <",
        env!("CARGO_PKG_HOMEPAGE"),
        "> for details.\n"
    ));

    let result = yash_builtin::common::output(env, &help).await;
    env.exit_status = result.exit_status();
}

async fn print_version<S>(env: &mut Env<S>)
where
    S: Isatty + WriteAll,
{
    let version = env!("CARGO_PKG_VERSION");
    let result = yash_builtin::common::output(env, &format!("yash {version}\n")).await;
    env.exit_status = result.exit_status();
}

#[allow(
    clippy::await_holding_refcell_ref,
    reason = "`print_error` does not run concurrently with the input decorators or read-eval loop"
)]
async fn run_as_shell_process<S>(env: &mut Env<S>)
where
    S: Chdir
        + Clone
        + GetCwd
        + GetRlimit
        + GetUid
        + Runtime
        + Sysconf
        + TcGetPgrp
        + Times
        + Umask
        + Write
        + 'static,
{
    // Parse the command-line arguments
    let run = match self::startup::args::parse(std::env::args()) {
        Ok(Parse::Help) => return print_help(env).await,
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
    let portable = run
        .options
        .iter()
        .rev()
        .find_map(|&(option, state)| (option == Portable).then_some(state))
        == Some(On);
    env.variables.extend_env(
        std::env::vars()
            .filter(|(name, _)| !portable || yash_env::variable::is_portable_variable_name(name)),
    );

    let work = self::startup::configure_environment(env, run).await;

    let is_interactive = env.options.get(Interactive) == On;

    // Run initialization files
    // TODO run profile if login (but not the default profile if portable)
    run_rcfile(env, work.rcfile).await;

    // Prepare the input for the main read-eval loop
    let ref_env = RefCell::new(env);
    let lexer = match prepare_input(&ref_env, &work.source).await {
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

    // Rust by default sets SIGPIPE to SIG_IGN, which is not desired.
    // As an imperfect workaround, we set SIGPIPE to SIG_DFL here.
    // TODO Use unix_sigpipe: https://github.com/rust-lang/rust/issues/97889
    system
        .sigaction(RealSystem::SIGPIPE, Disposition::Default)
        .ok();

    let system = Rc::new(Concurrent::new(system));
    let runner = Rc::clone(&system);
    let task = async {
        let mut env = Env::with_system(system);
        run_as_shell_process(&mut env).await;
        exit_or_raise(&env.system, env.exit_status).await
    };
    runner.run_real(task)
}
