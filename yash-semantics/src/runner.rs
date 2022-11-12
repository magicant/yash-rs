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

use crate::trap::run_traps_for_caught_signals;
use crate::Command;
use crate::Handle;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::parser::Parser;

/// Reads and executes commands repeatedly.
///
/// This function uses `lexer` for reading and parsing input and `env` for
/// executing parsed commands. If [`Parser::command_line`] parses successfully,
/// the command is executed, and then the next command line is parsed. The loop
/// continues until the parser reaches the end of input or encounters a syntax
/// error, or the command execution results in a `Break(Divert::...)`.
///
/// If the input source code contains no commands, the exit status is set to
/// zero.
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
/// # use std::ops::ControlFlow::Continue;
/// # use yash_env::Env;
/// # use yash_semantics::*;
/// # use yash_syntax::parser::lex::Lexer;
/// # use yash_syntax::source::Source;
/// let mut env = Env::new_virtual();
/// let mut lexer = Lexer::from_memory("case foo in (bar) ;; esac", Source::Unknown);
/// let result = read_eval_loop(&mut env, &mut lexer).await;
/// assert_eq!(result, Continue(()));
/// assert_eq!(env.exit_status, ExitStatus::SUCCESS);
/// # })
/// ```
pub async fn read_eval_loop(env: &mut Env, lexer: &mut Lexer<'_>) -> Result {
    let mut executed = false;

    loop {
        if !lexer.pending() {
            lexer.flush();
        }
        let mut parser = Parser::new(lexer, &env.aliases);
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

/// Like [`read_eval_loop`], but returns the future in a pinned box.
pub fn read_eval_loop_boxed<'a>(
    env: &'a mut Env,
    lexer: &'a mut Lexer<'_>,
) -> Pin<Box<dyn Future<Output = Result> + 'a>> {
    Box::pin(read_eval_loop(env, lexer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::semantics::Divert;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::trap::Signal;
    use yash_env::trap::Trap;
    use yash_syntax::source::Location;
    use yash_syntax::source::Source;

    #[test]
    fn exit_status_zero_with_no_commands() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(5);
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn exit_status_in_out() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.exit_status = ExitStatus(42);
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let mut lexer = Lexer::from_memory("echo $?; return -n 7", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(7));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "42\n"));
    }

    #[test]
    fn executing_many_lines_of_code() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let mut lexer = Lexer::from_memory("echo 1\necho 2\necho 3;", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
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
        let mut lexer = Lexer::from_memory("echo", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "alias\nok\n"));
    }

    #[test]
    fn handling_syntax_error() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut lexer = Lexer::from_memory(";;", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn syntax_error_aborts_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let mut lexer = Lexer::from_memory(";;\necho !", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
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
            .set_trap(
                &mut env.system,
                Signal::SIGUSR1,
                Trap::Command("echo USR1".into()),
                Location::dummy(""),
                false,
            )
            .unwrap();
        state
            .borrow_mut()
            .processes
            .get_mut(&system.process_id)
            .unwrap()
            .raise_signal(Signal::SIGUSR1);
        let mut lexer = Lexer::from_memory("echo $?", Source::Unknown);
        let result = read_eval_loop(&mut env, &mut lexer).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "USR1\n0\n"));
    }
}
