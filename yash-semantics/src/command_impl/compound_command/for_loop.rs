// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Execution of the for loop

use std::ops::ControlFlow::Continue;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax::List;
use yash_syntax::syntax::Word;

/// Executes the for loop.
pub async fn execute(
    _env: &mut Env,
    _name: &Word,
    _values: &Option<Vec<Word>>,
    _body: &List,
) -> Result {
    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::Command;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::semantics::ExitStatus;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::variable::Value::Array;
    use yash_env::VirtualSystem;
    use yash_syntax::syntax::CompoundCommand;

    #[test]
    fn without_words_without_positional_parameters() {
        let mut env = Env::new_virtual();
        let command: CompoundCommand = "for v do unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    #[ignore]
    fn without_words_with_one_positional_parameters() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.variables.positional_params_mut().value = Array(vec!["foo".to_string()]);
        let command: CompoundCommand = "for v do echo :$v:; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        let file = state.borrow().file_system.get("/dev/stdout").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content).unwrap(), ":foo:\n");
        });
    }

    // TODO without_words_with_many_positional_parameters
    // TODO with_one_word
    // TODO with_many_words
    // TODO with empty body
    // TODO break_for_loop
    // TODO break_outer_loop
    // TODO continue_for_loop
    // TODO continue_outer_loop
}
