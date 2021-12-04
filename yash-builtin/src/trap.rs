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
//! TODO Elaborate

use crate::common::Print;
use crate::common::Stderr;
use crate::common::Stdout;
use std::fmt::Write;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::trap::Iter;
use yash_env::trap::SetTrapError;
use yash_env::trap::Signal;
use yash_env::trap::Trap;
use yash_env::trap::TrapState;
use yash_quote::quote;
use yash_syntax::source::Location;

/// Part of the shell execution environment the trap built-in depends on.
pub trait Env: Stdout + Stderr {
    /// Returns an iterator for currently configured trap actions.
    fn iter(&self) -> Iter;

    /// Returns the trap action for a signal.
    ///
    /// This function returns a pair of optional trap states. The first is the
    /// currently configured trap action, and the second is the action set
    /// before entering the current subshell environment.
    ///
    /// This function does not reflect the initial signal actions the shell
    /// inherited on startup.
    fn get_trap(&self, signal: Signal) -> (Option<&TrapState>, Option<&TrapState>);

    /// Sets a trap action for a signal.
    ///
    /// This function installs a signal handler to the specified underlying
    /// system.
    ///
    /// If `override_ignore` is `false`, you cannot set a trap for a signal that
    /// has been ignored since the shell startup. An interactive shell should
    /// set `override_ignore` to `true` to bypass this restriction.
    ///
    /// You can never set a trap for `SIGKILL` or `SIGSTOP`.
    ///
    /// `origin` should be the location of the command performing this trap
    /// update. It is only informative: It does not affect the signal handling
    /// behavior and can be referenced later by [`get_trap`](Self::get_trap).
    fn set_trap(
        &mut self,
        signal: Signal,
        action: Trap,
        origin: Location,
        override_ignore: bool,
    ) -> std::result::Result<(), SetTrapError>;
}

impl Env for yash_env::Env {
    fn iter(&self) -> Iter {
        self.traps.iter()
    }

    fn get_trap(&self, signal: Signal) -> (Option<&TrapState>, Option<&TrapState>) {
        self.traps.get_trap(signal)
    }

    fn set_trap(
        &mut self,
        signal: Signal,
        action: Trap,
        origin: Location,
        override_ignore: bool,
    ) -> std::result::Result<(), SetTrapError> {
        self.traps
            .set_trap(&mut self.system, signal, action, origin, override_ignore)
    }
}

/// Prints the currently configured traps.
pub async fn print_traps<E: Env>(env: &mut E) -> Result {
    let mut output = String::new();
    for (&signal, current, parent) in env.iter() {
        let trap = match (current, parent) {
            (Some(trap), _) => trap,
            (None, Some(trap)) => trap,
            (None, None) => continue,
        };
        let command = match &trap.action {
            Trap::Default => continue,
            Trap::Ignore => "",
            Trap::Command(command) => command,
        };
        let signal = &signal.as_str()[3..];
        writeln!(output, "trap -- {} {}", &quote(command), signal).ok();
    }
    (env.print(&output).await, Continue(()))
}

/// Implementation of the readonly built-in.
pub async fn builtin_body<E: Env>(env: &mut E, mut args: Vec<Field>) -> Result {
    if args.len() != 3 {
        // TODO Support full syntax
        if args.len() == 1 {
            return print_traps(env).await;
        }
        return (ExitStatus::ERROR, Continue(()));
    }

    let Field { value, origin } = args.remove(1);

    let signal_name = format!("SIG{}", args[1].value);
    // TODO Support real-time signals
    let signal = match signal_name.parse() {
        Ok(signal) => signal,
        // TODO Print error message for the unknown signal
        Err(_) => return (ExitStatus::FAILURE, Continue(())),
    };
    let action = match value.as_str() {
        "-" => Trap::Default,
        "" => Trap::Ignore,
        _ => Trap::Command(value),
    };

    match env.set_trap(signal, action, origin, false) {
        Ok(()) => (ExitStatus::SUCCESS, Continue(())),
        // TODO Print error message
        Err(_) => (ExitStatus::ERROR, Continue(())),
    }
}

/// Implementation of the trap built-in.
///
/// This function calls [`builtin_body`] and wraps the result in a pinned box.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;
    use futures_util::future::FutureExt;
    use std::rc::Rc;
    use yash_env::io::Fd;
    use yash_env::system::SignalHandling;
    use yash_env::Env;
    use yash_env::VirtualSystem;

    #[test]
    fn setting_trap_to_ignore() {
        let system = Box::new(VirtualSystem::new());
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "", "USR1"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let process = &state.borrow().processes[&pid];
        assert_eq!(
            process.signal_handling(Signal::SIGUSR1),
            SignalHandling::Ignore
        );
    }

    #[test]
    fn setting_trap_to_command() {
        let system = Box::new(VirtualSystem::new());
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "echo", "USR2"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let process = &state.borrow().processes[&pid];
        assert_eq!(
            process.signal_handling(Signal::SIGUSR2),
            SignalHandling::Catch
        );
    }

    #[test]
    fn resetting_trap() {
        let system = Box::new(VirtualSystem::new());
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "-", "PIPE"]);
        let result = builtin_body(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let process = &state.borrow().processes[&pid];
        assert_eq!(
            process.signal_handling(Signal::SIGPIPE),
            SignalHandling::Default
        );
    }

    #[test]
    fn printing_no_trap() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap"]);

        let result = block_on(builtin_body(&mut env, args));
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, []);
    }

    #[test]
    fn printing_some_trap() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "echo", "INT"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap"]);

        let result = block_on(builtin_body(&mut env, args));
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, b"trap -- echo INT\n");
    }

    #[test]
    fn printing_some_traps() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "echo", "INT"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap", "echo t", "TERM"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap"]);

        let result = block_on(builtin_body(&mut env, args));
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, b"trap -- echo INT\ntrap -- 'echo t' TERM\n");
    }

    #[test]
    fn error_printing_traps() {
        let mut system = Box::new(VirtualSystem::new());
        system.current_process_mut().close_fd(Fd::STDOUT);
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "echo", "INT"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap"]);

        let result = block_on(builtin_body(&mut env, args));
        assert_eq!(result, (ExitStatus::FAILURE, Continue(())));
        let state = state.borrow();
        let file = state.file_system.get("/dev/stderr").unwrap().borrow();
        assert_ne!(file.content, []);
    }

    #[test]
    fn printing_traps_in_subshell() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "echo", "INT"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap", "", "TERM"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        env.traps.enter_subshell(&mut env.system);
        let args = Field::dummies(["trap"]);

        let result = block_on(builtin_body(&mut env, args));
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, b"trap -- echo INT\ntrap -- '' TERM\n");
    }

    #[test]
    fn printing_traps_after_setting_in_subshell() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let args = Field::dummies(["trap", "echo", "INT"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap", "", "TERM"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        env.traps.enter_subshell(&mut env.system);
        let args = Field::dummies(["trap", "ls", "QUIT"]);
        let _ = builtin_body(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["trap"]);

        let result = block_on(builtin_body(&mut env, args));
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));
        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, b"trap -- ls QUIT\ntrap -- '' TERM\n");
    }
}
