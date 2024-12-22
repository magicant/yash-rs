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

use crate::assign::Error;
use crate::assign::ErrorCause;
use crate::command::Command;
use crate::expansion::expand_word;
use crate::expansion::expand_words;
use crate::expansion::AssignReadOnlyError;
use crate::xtrace::print;
use crate::xtrace::trace_fields;
use crate::xtrace::XTrace;
use crate::Handle;
use std::fmt::Write;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::semantics::Result;
use yash_env::stack::Frame;
use yash_env::variable::Scope;
use yash_env::Env;
use yash_quote::quoted;
use yash_syntax::syntax::List;
use yash_syntax::syntax::Word;

/// Executes the for loop.
pub async fn execute(
    env: &mut Env,
    name: &Word,
    values: &Option<Vec<Word>>,
    body: &List,
) -> Result {
    let (name, _) = match expand_word(env, name).await {
        Ok(word) => word,
        Err(error) => return error.handle(env).await,
    };

    let values = if let Some(words) = values {
        match expand_words(env, words).await {
            Ok((fields, _)) => fields,
            Err(error) => return error.handle(env).await,
        }
    } else {
        env.variables
            .positional_params()
            .values
            .iter()
            .map(|value| Field {
                value: value.clone(),
                origin: name.origin.clone(),
            })
            .collect()
    };

    trace_values(env, &name, &values).await;

    let env = &mut env.push_frame(Frame::Loop);

    if values.is_empty() && !body.0.is_empty() {
        env.exit_status = ExitStatus::SUCCESS;
        return Continue(());
    }

    for Field { value, origin } in values {
        let mut var = env.get_or_create_variable(name.value.clone(), Scope::Global);
        match var.assign(value, origin) {
            Ok(_) => match body.execute(env).await {
                Break(Divert::Break { count: 0 }) => break,
                Break(Divert::Break { count }) => return Break(Divert::Break { count: count - 1 }),
                Break(Divert::Continue { count: 0 }) => continue,
                Break(Divert::Continue { count }) => {
                    return Break(Divert::Continue { count: count - 1 })
                }
                other => other?,
            },
            Err(error) => {
                let cause = ErrorCause::AssignReadOnly(AssignReadOnlyError {
                    name: name.value,
                    new_value: error.new_value,
                    read_only_location: error.read_only_location,
                    vacancy: None,
                });
                let location = name.origin;
                let error = Error { cause, location };
                return error.handle(env).await;
            }
        };
    }

    Continue(())
}

async fn trace_values(env: &mut Env, name: &Field, values: &[Field]) {
    if let Some(mut xtrace) = XTrace::from_options(&env.options) {
        write!(xtrace.words(), "for {} in ", quoted(&name.value)).unwrap();
        trace_fields(Some(&mut xtrace), values);
        print(env, xtrace).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::break_builtin;
    use crate::tests::continue_builtin;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use yash_env::builtin::Builtin;
    use yash_env::option::Option::ErrExit;
    use yash_env::option::State::On;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::CompoundCommand;

    #[test]
    fn without_words_without_positional_parameters() {
        let mut env = Env::new_virtual();
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "for v do unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
    }

    #[test]
    fn without_words_with_one_positional_parameters() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.variables.positional_params_mut().values = vec!["foo".to_string()];
        let command: CompoundCommand = "for v do echo :$v:; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ":foo:\n"));
    }

    #[test]
    fn without_words_with_many_positional_parameters() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.variables.positional_params_mut().values =
            vec!["1".to_string(), "2".to_string(), "three".to_string()];
        let command: CompoundCommand = "for foo do echo :$foo:; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ":1:\n:2:\n:three:\n"));
    }

    #[test]
    fn with_one_word() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "for v in 1; do echo :$v:; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, ":1:\n"));
    }

    #[test]
    fn with_many_words() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "for v in baz bar foo; do echo +$v+; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "+baz+\n+bar+\n+foo+\n"));
    }

    // TODO with empty body

    #[test]
    fn stack_frame_in_loop() {
        fn execute(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async move {
                assert_eq!(env.stack[0], Frame::Loop);
                Default::default()
            })
        }
        let mut env = Env::new_virtual();
        env.builtins.insert(
            "check",
            Builtin {
                r#type: yash_env::builtin::Type::Mandatory,
                execute,
                is_declaration_utility: Some(false),
            },
        );
        let command: CompoundCommand = "for i in 1; do check; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.stack[..], []);
    }

    #[test]
    fn xtrace_of_for_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.options.set(yash_env::option::Option::XTrace, On);
        let command: CompoundCommand = r"for i in 1 \$ 3; do echo $i; done".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "for i in 1 '$' 3\necho 1\necho '$'\necho 3\n");
        });
    }

    #[test]
    fn return_from_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let command = "for i in 1; do return -n 5; return 2; echo X; done";
        let command: CompoundCommand = command.parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(Some(ExitStatus(2)))));
        assert_eq!(env.exit_status, ExitStatus(5));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn break_for_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "for i in 1 2 3; do echo a; break; echo b; done"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "a\n"));
    }

    #[test]
    fn break_outer_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("break", break_builtin());
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "for i in 1 2 3; do echo a; break $n; echo b; done"
            .parse()
            .unwrap();

        for n in 2..5 {
            env.exit_status = ExitStatus(123);
            env.variables
                .get_or_new("n", Scope::Global)
                .assign(n.to_string(), None)
                .unwrap();

            let result = command.execute(&mut env).now_or_never().unwrap();
            assert_eq!(result, Break(Divert::Break { count: n - 2 }));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        }
        assert_stdout(&state, |stdout| assert_eq!(stdout, "a\na\na\n"));
    }

    #[test]
    fn continue_for_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        env.exit_status = ExitStatus(123);
        let command: CompoundCommand = "for i in 1 2 3; do echo +$i; continue; echo -$i; done"
            .parse()
            .unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "+1\n+2\n+3\n"));
    }

    #[test]
    fn continue_outer_loop() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("continue", continue_builtin());
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "for i in 1 2 3; do echo a; continue $n; echo b; done"
            .parse()
            .unwrap();

        for n in 2..5 {
            env.exit_status = ExitStatus(123);
            env.variables
                .get_or_new("n", Scope::Global)
                .assign(n.to_string(), None)
                .unwrap();

            let result = command.execute(&mut env).now_or_never().unwrap();
            assert_eq!(result, Break(Divert::Continue { count: n - 2 }));
            assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        }
        assert_stdout(&state, |stdout| assert_eq!(stdout, "a\na\na\n"));
    }

    #[test]
    fn expansion_error_in_name() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "for $() do echo unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn errexit_with_expansion_error_in_name() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.options.set(ErrExit, On);
        let command: CompoundCommand = "for $() do echo unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn expansion_error_in_words() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: CompoundCommand = "for x in $(); do echo unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn errexit_with_expansion_error_in_words() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.options.set(ErrExit, On);
        let command: CompoundCommand = "for x in $(); do echo unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn assignment_error_with_read_only_variable() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let mut var = env.variables.get_or_new("x", Scope::Global);
        var.assign("", None).unwrap();
        var.make_read_only(Location::dummy(""));
        let command: CompoundCommand = "for x in x; do echo unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }

    #[test]
    fn errexit_with_assignment_error_with_read_only_variable() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.options.set(ErrExit, On);
        let mut var = env.variables.get_or_new("x", Scope::Global);
        var.assign("", None).unwrap();
        var.make_read_only(Location::dummy(""));
        let command: CompoundCommand = "for x in x; do echo unreached; done".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Exit(Some(ExitStatus::ERROR))));
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
        assert_stderr(&state, |stderr| assert_ne!(stderr, ""));
    }
}
