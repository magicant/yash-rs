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

use crate::Command;
use crate::Handle;
use std::ops::ControlFlow::Continue;
use yash_env::exec::ExitStatus;
use yash_env::exec::Result;
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
/// TODO: `Break(Divert::Interrupt(...))` does not end the loop in an
/// interactive shell
///
/// TODO: Handle traps while reading commands
pub async fn read_eval_loop(env: &mut Env, lexer: &mut Lexer<'_>) -> Result {
    let mut executed = false;

    loop {
        let mut parser = Parser::new(lexer, &env.aliases);
        match parser.command_line().await {
            Ok(None) => break,
            Ok(Some(command)) => command.execute(env).await?,
            Err(error) => error.handle(env).await?,
        };
        executed = true;
    }

    if !executed {
        env.exit_status = ExitStatus::SUCCESS;
    }

    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{echo_builtin, return_builtin};
    use futures_executor::block_on;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::exec::Divert;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_syntax::source::Location;
    use yash_syntax::source::Source;

    #[test]
    fn exit_status_zero_with_no_commands() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(5);
        let mut lexer = Lexer::from_memory("", Source::Unknown);
        let result = block_on(read_eval_loop(&mut env, &mut lexer));
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
        let result = block_on(read_eval_loop(&mut env, &mut lexer));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(7));

        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, "42\n".as_bytes());
    }

    #[test]
    fn executing_many_lines_of_code() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let mut lexer = Lexer::from_memory("echo 1\necho 2\necho 3;", Source::Unknown);
        let result = block_on(read_eval_loop(&mut env, &mut lexer));
        assert_eq!(result, Continue(()));

        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, "1\n2\n3\n".as_bytes());
    }

    #[test]
    fn parsing_with_aliases() {
        use yash_syntax::alias::{Alias, HashEntry};
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        Rc::make_mut(&mut env.aliases).insert(HashEntry(Rc::new(Alias {
            name: "echo".to_string(),
            replacement: "echo alias\necho ok".to_string(),
            global: false,
            origin: Location::dummy(""),
        })));
        env.builtins.insert("echo", echo_builtin());
        let mut lexer = Lexer::from_memory("echo", Source::Unknown);
        let result = block_on(read_eval_loop(&mut env, &mut lexer));
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(file.content, "alias\nok\n".as_bytes());
    }

    #[test]
    fn handling_syntax_error() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut lexer = Lexer::from_memory(";;", Source::Unknown);
        let result = block_on(read_eval_loop(&mut env, &mut lexer));
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));

        let state = state.borrow();
        let file = state.file_system.get("/dev/stderr").unwrap().borrow();
        assert!(!file.content.is_empty());
    }

    #[test]
    fn syntax_error_aborts_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let mut lexer = Lexer::from_memory(";;\necho !", Source::Unknown);
        let result = block_on(read_eval_loop(&mut env, &mut lexer));
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));

        let state = state.borrow();
        let file = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert!(file.content.is_empty(), "stdout={:?}", file.content);
    }
}
