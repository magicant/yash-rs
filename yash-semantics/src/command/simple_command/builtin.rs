// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Simple command semantics for built-ins

use super::perform_assignments;
use crate::redir::RedirGuard;
use crate::xtrace::print;
use crate::xtrace::trace_fields;
use crate::xtrace::XTrace;
use crate::Handle;
use std::ops::ControlFlow::{Break, Continue};
use yash_env::builtin::Builtin;
use yash_env::semantics::Divert;
use yash_env::semantics::Field;
use yash_env::semantics::Result;
use yash_env::stack::Builtin as FrameBuiltin;
use yash_env::variable::Context;
use yash_env::Env;
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

pub async fn execute_builtin(
    env: &mut Env,
    builtin: Builtin,
    assigns: &[Assign],
    mut fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    use yash_env::builtin::Type::*;

    let mut xtrace = XTrace::from_options(&env.options);
    trace_fields(xtrace.as_mut(), &fields);

    let name = fields.remove(0);
    let is_special = builtin.r#type == Special;
    let env = &mut env.push_frame(FrameBuiltin { name, is_special }.into());

    let env = &mut RedirGuard::new(env);
    if let Err(e) = env.perform_redirs(redirs, xtrace.as_mut()).await {
        e.handle(env).await?;
        return match builtin.r#type {
            Special => Break(Divert::Interrupt(None)),
            Mandatory | Elective | Extension | Substitutive => Continue(()),
        };
    };

    let result = match builtin.r#type {
        Special => {
            perform_assignments(env, assigns, false, xtrace.as_mut()).await?;
            print(env, xtrace).await;
            (builtin.execute)(env, fields).await
        }
        // TODO Reject elective and extension built-ins in POSIX mode
        Mandatory | Elective | Extension | Substitutive => {
            let mut env = env.push_context(Context::Volatile);
            perform_assignments(&mut env, assigns, true, xtrace.as_mut()).await?;
            print(&mut env, xtrace).await;
            (builtin.execute)(&mut env, fields).await
        }
    };

    if result.should_retain_redirs() {
        env.preserve_redirs();
    }
    env.exit_status = result.exit_status();
    result.divert()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::tests::echo_builtin;
    use crate::tests::local_builtin;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::option::State::On;
    use yash_env::semantics::ExitStatus;
    use yash_env::stack::Frame;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::Errno;
    use yash_env::variable::Value;
    use yash_env::VirtualSystem;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_syntax::syntax;

    #[test]
    fn simple_command_returns_exit_status_from_builtin_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::SimpleCommand = "return -n 93".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus(93));
    }

    #[test]
    fn simple_command_returns_exit_status_from_builtin_with_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert(
            "foo",
            Builtin {
                r#type: yash_env::builtin::Type::Special,
                execute: |_env, _args| {
                    Box::pin(std::future::ready({
                        yash_env::builtin::Result::with_exit_status_and_divert(
                            ExitStatus(37),
                            Break(Divert::Return(None)),
                        )
                    }))
                },
                is_declaration_utility: Some(false),
            },
        );
        let command: syntax::SimpleCommand = "foo".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Return(None)));
        assert_eq!(env.exit_status, ExitStatus(37));
    }

    #[test]
    fn simple_command_applies_redirections_to_builtin() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: syntax::SimpleCommand = "echo hello >/tmp/file".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();

        let file = state.borrow().file_system.get("/tmp/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("hello\n"));
        });
    }

    #[test]
    fn simple_command_by_default_reverts_redirections_to_builtin() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: syntax::SimpleCommand = "echo hello >/tmp/file".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        let command: syntax::SimpleCommand = "echo world".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();

        assert_stdout(&state, |stdout| assert_eq!(stdout, "world\n"));
    }

    #[test]
    fn simple_command_retains_redirections_to_builtin_if_requested() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert(
            "exec",
            Builtin {
                r#type: yash_env::builtin::Type::Mandatory,
                execute: |_env, _args| {
                    Box::pin(async {
                        let mut result = yash_env::builtin::Result::default();
                        result.retain_redirs();
                        result
                    })
                },
                is_declaration_utility: Some(false),
            },
        );
        let command: syntax::SimpleCommand = "exec >/tmp/file".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        let command: syntax::SimpleCommand = "echo hello".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();

        let file = state.borrow().file_system.get("/tmp/file").unwrap();
        let file = file.borrow();
        assert_matches!(&file.body, FileBody::Regular { content, .. } => {
            assert_eq!(from_utf8(content), Ok("hello\n"));
        });
    }

    #[test]
    fn simple_command_skips_running_builtin_on_redirection_error() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let command: syntax::SimpleCommand = "echo X </no/such/file >/tmp/file".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
        assert_eq!(
            state.borrow().file_system.get("/tmp/file"),
            Err(Errno::ENOENT)
        );
        assert_stdout(&state, |stdout| assert_eq!(stdout, ""));
    }

    #[test]
    fn special_builtin_interrupts_on_redirection_error() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("return", return_builtin());
        let command: syntax::SimpleCommand = "return </no/such/file".parse().unwrap();

        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Break(Divert::Interrupt(None)));
        assert_eq!(env.exit_status, ExitStatus::ERROR);
    }

    #[test]
    fn simple_command_assigns_permanently_for_special_builtin() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let command: syntax::SimpleCommand = "v=42 return -n 0".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        let v = env.variables.get("v").unwrap();
        assert_eq!(v.value, Some(Value::scalar("42")));
        assert!(!v.is_exported);
    }

    #[test]
    fn simple_command_assigns_temporarily_for_regular_builtin() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("local", local_builtin());
        let command: syntax::SimpleCommand = "v=42 local v".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(env.variables.get("v"), None);
        assert_stdout(&state, |stdout| assert_eq!(stdout, "v=42\n"));
    }

    #[test]
    fn simple_command_pushes_stack_frame_for_builtin() {
        fn builtin_main(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async {
                assert_matches!(&env.stack[..], [Frame::Builtin(builtin)] => {
                    assert_eq!(builtin.name.value, "builtin");
                    assert!(!builtin.is_special);
                });
                Default::default()
            })
        }
        fn special_main(
            env: &mut Env,
            _args: Vec<Field>,
        ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
            Box::pin(async {
                assert_matches!(&env.stack[..], [Frame::Builtin(builtin)] => {
                    assert_eq!(builtin.name.value, "special");
                    assert!(builtin.is_special);
                });
                Default::default()
            })
        }

        let mut env = Env::new_virtual();
        env.builtins.insert(
            "builtin",
            Builtin {
                r#type: yash_env::builtin::Type::Mandatory,
                execute: builtin_main,
                is_declaration_utility: Some(false),
            },
        );
        env.builtins.insert(
            "special",
            Builtin {
                r#type: yash_env::builtin::Type::Special,
                execute: special_main,
                is_declaration_utility: Some(false),
            },
        );
        let command: syntax::SimpleCommand = "builtin".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        let command: syntax::SimpleCommand = "special".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(env.stack[..], []);
    }

    #[test]
    fn xtrace_for_builtin() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.options.set(yash_env::option::XTrace, On);
        let command: syntax::SimpleCommand = "foo=bar echo hello >/dev/null".parse().unwrap();
        command.execute(&mut env).now_or_never().unwrap();

        assert_stderr(&state, |stderr| {
            assert_eq!(stderr, "foo=bar echo hello 1>/dev/null\n");
        });
    }
}
