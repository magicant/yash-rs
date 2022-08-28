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

//! Execution of the case command

use crate::expansion::expand_word;
use crate::Command;
use std::ops::ControlFlow::Continue;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_env::Env;
use yash_fnmatch::without_escape;
use yash_fnmatch::Config;
use yash_fnmatch::Pattern;
use yash_syntax::syntax::CaseItem;
use yash_syntax::syntax::Word;

fn config() -> Config {
    let mut config = Config::default();
    config.anchor_begin = true;
    config.anchor_end = true;
    config
}

/// Executes the case command.
pub async fn execute(env: &mut Env, subject: &Word, items: &[CaseItem]) -> Result {
    let subject = match expand_word(env, subject).await {
        Ok((expansion, _exit_status)) => expansion,
        Err(error) => todo!("{:?}", error), // TODO return error.handle(env).await,
    };

    for item in items {
        for pattern in &item.patterns {
            // TODO Apply quotes in pattern
            let pattern = match expand_word(env, pattern).await {
                Ok((expansion, _exit_status)) => expansion,
                Err(error) => todo!("{:?}", error), // TODO return error.handle(env).await,
            };

            let pattern = match Pattern::parse_with_config(without_escape(&pattern.value), config())
            {
                Ok(parse) => parse,
                Err(error) => todo!("ignore broken pattern: {:?}", error),
            };
            if pattern.is_match(&subject.value) {
                return item.body.execute(env).await;
            }
        }
    }

    env.exit_status = ExitStatus::SUCCESS;
    Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use crate::Command;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::VirtualSystem;
    use yash_syntax::syntax::CompoundCommand;

    fn fixture() -> (Env, Rc<RefCell<SystemState>>) {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        (env, state)
    }

    #[test]
    fn no_items() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(57);
        let command: CompoundCommand = "case foo in esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn one_unmatched_item() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus(17);
        let command: CompoundCommand = "case foo in (bar) echo X;; esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn many_unmatched_items() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus::FAILURE;
        let command: CompoundCommand = "case word in
        (foo) echo foo;;
        (bar|baz) echo bar baz;;
        (1|2|3) echo 1 2 3;;
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn first_item_matched() {
        in_virtual_system(|mut env, _pid, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            env.exit_status = ExitStatus(100);
            let command: CompoundCommand = "case foo in
            ($(echo foo)) echo A; return -n 42;;
            ($(echo 1 >&2)|$(echo 2 >&2)|$(echo 3 >&2)) echo B;;
            ($(echo 4 >&2)|$(echo 5 >&2)) echo C;;
            esac"
                .parse()
                .unwrap();

            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus(42));
            assert_stdout(&state, |stdout| assert_eq!(stdout, "A\n"));
            assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
        })
    }

    #[test]
    fn first_pattern_of_second_item_matched() {
        in_virtual_system(|mut env, _pid, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.exit_status = ExitStatus(100);
            let command: CompoundCommand = "case 1 in
            ($(echo foo)) echo A;;
            ($(echo 1)|$(echo 2)|$(echo 3 >&2)) echo B;;
            ($(echo 4 >&2)|$(echo 5 >&2)) echo C;;
            esac"
                .parse()
                .unwrap();

            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            assert_stdout(&state, |stdout| assert_eq!(stdout, "B\n"));
            assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
        })
    }

    #[test]
    fn second_pattern_of_second_item_matched() {
        in_virtual_system(|mut env, _pid, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.exit_status = ExitStatus(100);
            let command: CompoundCommand = "case 2 in
            ($(echo foo)) echo A;;
            ($(echo 1)|$(echo 2)|$(echo 3 >&2)) echo B;;
            ($(echo 4 >&2)|$(echo 5 >&2)) echo C;;
            esac"
                .parse()
                .unwrap();

            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            assert_stdout(&state, |stdout| assert_eq!(stdout, "B\n"));
            assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
        })
    }

    #[test]
    fn third_item_matched() {
        in_virtual_system(|mut env, _pid, state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.exit_status = ExitStatus(100);
            let command: CompoundCommand = "case 4 in
            ($(echo foo)) echo A; return -n 42;;
            ($(echo 1)|$(echo 2)|$(echo 3)) echo B;;
            ($(echo 4)|$(echo 5 >&2)) echo C;;
            esac"
                .parse()
                .unwrap();

            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
            assert_stdout(&state, |stdout| assert_eq!(stdout, "C\n"));
            assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
        })
    }

    #[test]
    fn pattern_must_match_whole_word() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case 123 in
        (2) echo 2;;
        (2*) echo '2*';;
        (*2) echo '*2';;
        (1*3) echo '1*3';;
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1*3\n"));
    }

    // TODO Empty body
    // TODO Return from body
}
