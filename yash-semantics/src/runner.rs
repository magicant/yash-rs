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

//! Implementation of the read-eval loop

use crate::command::Command;
use crate::trap::run_traps_for_caught_signals;
use crate::Handle;
use std::cell::Cell;
use std::cell::RefCell;
use std::ops::ControlFlow::Continue;
use std::rc::Rc;
use yash_env::option::Option::Verbose;
use yash_env::option::State;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::parser::Parser;

/// Read-eval-loop
///
/// A read-eval-loop uses a [`Lexer`] for reading and parsing input and [`Env`]
/// for executing parsed commands. It creates a [`Parser`] from the lexer to
/// parse [command lines](Parser::command_line). The loop executes each command
/// line before parsing the following command line. The loop continues until the
/// parser reaches the end of input or encounters a syntax error, or the command
/// execution results in a `Break(Divert::...)`.
///
/// The loop takes a `&RefCell<&mut Env>` rather than a `&mut Env` to allow
/// the lexer and parser to borrow the environment mutably while they read and
/// parse input. The loop also needs to borrow the environment mutably to
/// execute commands, so the lexer and parser must release the borrow before
/// they return the result of parsing input.
///
/// If the input source code contains no commands, the exit status is set to
/// zero. Otherwise, the exit status reflects the result of the last executed
/// command.
///
/// [Pending traps are run](run_traps_for_caught_signals) and [subshell statuses
/// are updated](Env::update_all_subshell_statuses) between parsing input and
/// running commands.
///
/// TODO: `Break(Divert::Interrupt(...))` should not end the loop in an
/// interactive shell
///
/// # Example
///
/// ```
/// # futures_executor::block_on(async {
/// # use std::cell::RefCell;
/// # use std::ops::ControlFlow::Continue;
/// # use yash_env::Env;
/// # use yash_semantics::*;
/// # use yash_syntax::parser::lex::Lexer;
/// # use yash_syntax::source::Source;
/// let mut env = Env::new_virtual();
/// let shared_env = RefCell::new(&mut env);
/// let mut lexer = Lexer::from_memory("case foo in (bar) ;; esac", Source::Unknown);
/// let result = ReadEvalLoop::new(&shared_env, &mut lexer).run().await;
/// assert_eq!(result, Continue(()));
/// assert_eq!(env.exit_status, ExitStatus::SUCCESS);
/// # })
/// ```
#[derive(Debug)]
pub struct ReadEvalLoop<'a, 'b, 'c> {
    env: &'a RefCell<&'b mut Env>,
    lexer: &'a mut Lexer<'c>,
    verbose: Option<Rc<Cell<State>>>,
}

impl<'a, 'b, 'c> ReadEvalLoop<'a, 'b, 'c> {
    /// Creates a new read-eval-loop instance.
    ///
    /// This constructor requires two parameters: an environment in which the
    /// loop runs and a lexer that reads input.
    #[must_use]
    pub fn new(env: &'a RefCell<&'b mut Env>, lexer: &'a mut Lexer<'c>) -> Self {
        Self {
            env,
            lexer,
            verbose: None,
        }
    }

    /// Sets a shared option state to which the verbose option is reflected.
    ///
    /// This function is meant to be used with a lexer with an [`FdReader`]
    /// input. You should set the same shared cell of an option state to the
    /// input function and the loop. Before reading each command line, the loop
    /// copies the value of `env.options.get(Verbose)` to the cell. The input
    /// function checks it to see if it needs to echo the line it reads to the
    /// standard error. That achieves the effect of the `Verbose` shell option.
    ///
    /// ```
    /// # futures_executor::block_on(async {
    /// # use std::cell::{Cell, RefCell};
    /// # use std::num::NonZeroU64;
    /// # use std::rc::Rc;
    /// # use yash_env::Env;
    /// # use yash_env::input::FdReader;
    /// # use yash_env::io::Fd;
    /// # use yash_env::option::{Verbose, State};
    /// # use yash_semantics::*;
    /// # use yash_syntax::parser::lex::Lexer;
    /// # use yash_syntax::source::Source;
    /// let mut env = Env::new_virtual();
    /// let mut input = Box::new(FdReader::new(Fd::STDIN, Clone::clone(&env.system)));
    /// let verbose = Rc::new(Cell::new(State::Off));
    /// input.set_echo(Some(Rc::clone(&verbose)));
    /// let line = NonZeroU64::new(1).unwrap();
    /// let shared_env = RefCell::new(&mut env);
    /// let mut lexer = Lexer::new(input, line, Source::Stdin);
    /// let mut rel = ReadEvalLoop::new(&shared_env, &mut lexer);
    /// rel.set_verbose(Some(Rc::clone(&verbose)));
    /// let _ = rel.run().await;
    /// # })
    /// ```
    ///
    /// [`FdReader`]: yash_env::input::FdReader
    pub fn set_verbose(&mut self, verbose: Option<Rc<Cell<State>>>) {
        self.verbose = verbose;
    }

    /// Runs the read-eval-loop.
    #[allow(clippy::await_holding_refcell_ref)]
    pub async fn run(self) -> Result {
        // TODO More fine-grained borrow
        let env = &mut **self.env.borrow_mut();

        let mut executed = false;

        loop {
            if !self.lexer.pending() {
                self.lexer.flush();
            }
            if let Some(verbose) = &self.verbose {
                verbose.set(env.options.get(Verbose));
            }

            let mut parser = Parser::new(self.lexer, &env.aliases);
            match parser.command_line().await {
                Ok(Some(command)) => {
                    run_traps_for_caught_signals(env).await?;
                    env.update_all_subshell_statuses();
                    command.execute(env).await?
                }
                Ok(None) => break,
                Err(error) => error.handle(env).await?,
            };
            executed = true;
        }

        if !executed {
            env.exit_status = ExitStatus::SUCCESS;
        }

        Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::cell::Cell;
    use std::num::NonZeroU64;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::input::FdReader;
    use yash_env::io::Fd;
    use yash_env::option::Option::Verbose;
    use yash_env::option::State::{Off, On};
    use yash_env::semantics::Divert;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::system::r#virtual::SIGUSR1;
    use yash_env::trap::Action;
    use yash_syntax::source::Location;
    use yash_syntax::source::Source;

    #[test]
    fn exit_status_zero_with_no_commands() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(5);
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.borrow().exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn exit_status_in_out() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.exit_status = ExitStatus(42);
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory("echo $?; return -n 7", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.borrow().exit_status, ExitStatus(7));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "42\n"));
    }

    #[test]
    fn executing_many_lines_of_code() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory("echo 1\necho 2\necho 3;", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1\n2\n3\n"));
    }

    #[test]
    fn parsing_with_aliases() {
        use yash_syntax::alias::{Alias, HashEntry};
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.aliases.insert(HashEntry(Rc::new(Alias {
            name: "echo".to_string(),
            replacement: "echo alias\necho ok".to_string(),
            global: false,
            origin: Location::dummy(""),
        })));
        env.builtins.insert("echo", echo_builtin());
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory("echo", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.borrow().exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "alias\nok\n"));
    }

    #[test]
    fn verbose_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        state
            .borrow_mut()
            .file_system
            .get("/dev/stdin")
            .unwrap()
            .borrow_mut()
            .body = FileBody::new(*b"case _ in esac\n");
        let mut env = Env::with_system(Box::new(system));
        env.options.set(Verbose, On);
        let mut input = Box::new(FdReader::new(Fd::STDIN, Clone::clone(&env.system)));
        let verbose = Rc::new(Cell::new(Off));
        input.set_echo(Some(Rc::clone(&verbose)));
        let line = NonZeroU64::new(1).unwrap();
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::new(input, line, Source::Stdin);
        let mut rel = ReadEvalLoop::new(&env, &mut lexer);
        rel.set_verbose(Some(Rc::clone(&verbose)));

        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.borrow().exit_status, ExitStatus::SUCCESS);
        assert_eq!(verbose.get(), On);
        assert_stderr(&state, |stderr| assert_eq!(stderr, "case _ in esac\n"));
    }

    #[test]
    fn handling_syntax_error() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory(";;", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn syntax_error_aborts_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory(";;\necho !", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn running_traps_between_parsing_and_executing() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system.clone()));
        env.builtins.insert("echo", echo_builtin());
        env.traps
            .set_action(
                &mut env.system,
                SIGUSR1,
                Action::Command("echo USR1".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        let _ = state
            .borrow_mut()
            .processes
            .get_mut(&system.process_id)
            .unwrap()
            .raise_signal(SIGUSR1);
        let env = RefCell::new(&mut env);
        let mut lexer = Lexer::from_memory("echo $?", Source::Unknown);
        let rel = ReadEvalLoop::new(&env, &mut lexer);
        let result = rel.run().now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.borrow().exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "USR1\n0\n"));
    }
}
