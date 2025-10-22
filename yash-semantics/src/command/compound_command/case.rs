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

use crate::Handle;
use crate::command::Command;
use crate::expansion::attr_fnmatch::apply_escapes;
use crate::expansion::attr_fnmatch::to_pattern_chars;
use crate::expansion::expand_word;
use crate::expansion::expand_word_attr;
use crate::xtrace::XTrace;
use crate::xtrace::print;
use std::fmt::Write;
use std::ops::ControlFlow::Continue;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Result;
use yash_fnmatch::Config;
use yash_fnmatch::Pattern;
use yash_quote::quoted;
use yash_syntax::syntax::CaseItem;
use yash_syntax::syntax::Word;

async fn trace_subject(env: &mut Env, value: &str) {
    if let Some(mut xtrace) = XTrace::from_options(&env.options) {
        write!(xtrace.words(), "case {} in ", quoted(value)).unwrap();
        print(env, xtrace).await;
    }
}
// We don't trace expanded patterns since they need a quoting method different
// from yash_quote::quote.

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
        Err(error) => return error.handle(env).await,
    };
    trace_subject(env, &subject.value).await;

    let mut falling_through = false;
    let mut exit_status_updated = false;
    for item in items {
        if !falling_through {
            match matches(env, &subject.value, &item.patterns).await {
                Ok(true) => (),
                Ok(false) => continue,
                Err(error) => return error.handle(env).await,
            }
        }

        item.body.execute(env).await?;
        exit_status_updated = !item.body.0.is_empty();

        use yash_syntax::syntax::CaseContinuation::*;
        falling_through = match item.continuation {
            Break => break,
            FallThrough => true,
            Continue => false,
        };
    }

    if !exit_status_updated {
        env.exit_status = ExitStatus::SUCCESS;
    }
    Continue(())
}

/// Returns whether the subject matches any of the patterns.
///
/// Each pattern is expanded and matched against the subject.
/// Returns the error if any expansion fails.
async fn matches(
    env: &mut Env,
    subject: &str,
    patterns: &[Word],
) -> crate::expansion::Result<bool> {
    for pattern in patterns {
        let mut pattern = expand_word_attr(env, pattern).await?.0.chars;

        // Unquoted backslashes should act as quoting, as required by POSIX XCU 2.13.1
        apply_escapes(&mut pattern);

        let Ok(pattern) = Pattern::parse_with_config(to_pattern_chars(&pattern), config()) else {
            // Treat the broken pattern as a valid pattern that does not match anything
            continue;
        };

        if pattern.is_match(subject) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::option::Option::ErrExit;
    use yash_env::option::State::On;
    use yash_env::semantics::Divert;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::variable::Scope;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_env_test_helper::in_virtual_system;
    use yash_syntax::syntax::CompoundCommand;

    fn fixture() -> (Env, Rc<RefCell<SystemState>>) {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
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
        in_virtual_system(|mut env, state| async move {
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
        in_virtual_system(|mut env, state| async move {
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
        in_virtual_system(|mut env, state| async move {
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
        in_virtual_system(|mut env, state| async move {
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

    #[test]
    fn quoted_pattern() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case X in
        ('*') echo quoted;;
        (*) echo literal;;
        esac"
            .parse()
            .unwrap();

        let _ = command.execute(&mut env).now_or_never().unwrap();
        assert_stdout(&state, |stdout| assert_eq!(stdout, "literal\n"));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn unquoted_backslash_escapes_next_char_in_pattern() {
        let (mut env, state) = fixture();
        let var = &mut env.variables.get_or_new("empty", Scope::Global);
        var.assign("", None).unwrap();
        let var = &mut env.variables.get_or_new("one", Scope::Global);
        var.assign(r"\", None).unwrap();
        let var = &mut env.variables.get_or_new("v", Scope::Global);
        var.assign(r"\\\a", None).unwrap();
        let command: CompoundCommand = r#"case '\a' in
        ($empty) echo unquoted empty;;
        ("$empty") echo quoted empty;;
        ($one) echo unquoted one;;
        ("$one") echo quoted one;;
        ("$v") echo wrong quote;;
        ($v) echo ok;;
        esac"#
            .parse()
            .unwrap();

        let _ = command.execute(&mut env).now_or_never().unwrap();
        assert_stdout(&state, |stdout| assert_eq!(stdout, "ok\n"));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn broken_pattern_is_ignored() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case [[..]] in ([[..]]) echo X; esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn item_with_empty_body() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus::ERROR;
        let command: CompoundCommand = "case success in
        (success) ;;
        (success) echo X;;
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn breaking_terminator() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case 1 in
        (1) echo 1;;
        (1) echo 2;;
        (1) echo 3;;
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1\n"));
    }

    #[test]
    fn falling_through_terminator() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case foo in
        (x)   echo x;&
        (foo) echo 1;&
        (bar) echo 2; return -n 1;&
        (y)   ;;
        (foo) echo not reached;;
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1\n2\n"));
    }

    #[test]
    fn continuing_terminator() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case foo in
        (x)   echo x;|
        (foo) echo 1;|
        (boo) echo not reached 1;|
        (foo) ;|
        (foo) echo 2; return -n 42;|
        (boo) echo not reached 2;|
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(42));
        assert_stdout(&state, |stdout| assert_eq!(stdout, "1\n2\n"));
    }

    #[test]
    fn return_from_body() {
        let (mut env, state) = fixture();
        env.exit_status = ExitStatus::ERROR;
        let command: CompoundCommand = "case success in
        (success) return 73;;
        (success) echo X;;
        esac"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(73)))));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn xtrace_of_case() {
        let (mut env, state) = fixture();
        env.options.set(yash_env::option::Option::XTrace, On);
        let command: CompoundCommand =
            "case ${unset-X} in (foo);; (bar | ${unset-X} | not_reached) esac"
                .parse()
                .unwrap();
        _ = command.execute(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "case X in\n"));
    }

    #[test]
    fn error_expanding_subject() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case $(echo X) in (X) echo X; esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn errexit_with_error_expanding_subject() {
        let (mut env, state) = fixture();
        env.options.set(ErrExit, On);
        let command: CompoundCommand = "case $(echo X) in (X) echo X; esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn error_expanding_pattern() {
        let (mut env, state) = fixture();
        let command: CompoundCommand = "case X in ($(echo X)) echo X; esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn errexit_with_error_expanding_pattern() {
        let (mut env, state) = fixture();
        env.options.set(ErrExit, On);
        let command: CompoundCommand = "case X in ($(echo X)) echo X; esac".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }
}
