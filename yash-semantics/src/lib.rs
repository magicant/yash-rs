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

//! Semantics of the shell language.
//!
//! This crate defines the standard semantics for the shell language. The core
//! of the semantics is command execution and word expansion.
//! A command can be executed by calling [`Command::execute`].
//! A word can be expanded by using functions and traits defined in
//! [`expansion`].
//!
//! The [`read_eval_loop`] function reads, parses, and executes commands from an
//! input. It is a utility function that calls `Command::execute` for you.

pub mod assign;
mod command_impl;
pub mod command_search;
pub mod expansion;
mod handle_impl;
pub mod redir;
mod runner;
pub mod trap;

use annotate_snippets::display_list::DisplayList;
use annotate_snippets::snippet::Snippet;
use async_trait::async_trait;
use std::borrow::Cow;
use yash_env::io::Fd;
use yash_env::Env;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;

#[doc(no_inline)]
pub use yash_env::semantics::*;

/// Syntactic construct that can be executed.
#[async_trait(?Send)]
pub trait Command {
    /// Executes this command.
    ///
    /// TODO Elaborate: The exit status must be updated during execution.
    async fn execute(&self, env: &mut Env) -> Result;
}

/// Error handler.
///
/// Most errors in the shell are handled by printing an error message to the
/// standard error and returning a non-zero exit status. This trait provides a
/// standard interface for implementing that behavior.
#[async_trait(?Send)]
pub trait Handle {
    /// Handles the argument error.
    async fn handle(&self, env: &mut Env) -> Result;
}

/// Convenience function for printing an error message.
pub async fn print_error(
    env: &mut Env,
    title: Cow<'_, str>,
    label: Cow<'_, str>,
    location: &Location,
) {
    let mut a = vec![Annotation {
        r#type: AnnotationType::Error,
        label,
        location: location.clone(),
    }];
    location.code.source.complement_annotations(&mut a);
    let message = Message {
        r#type: AnnotationType::Error,
        title,
        annotations: a,
    };
    let mut snippet = Snippet::from(&message);
    snippet.opt.color = true;
    let s = format!("{}\n", DisplayList::from(snippet));
    let _ = env.system.write_all(Fd::STDERR, s.as_bytes()).await;
}

pub use runner::read_eval_loop;
pub use runner::read_eval_loop_boxed;

#[cfg(test)]
pub(crate) mod tests {
    use futures_executor::LocalSpawner;
    use futures_util::task::LocalSpawnExt;
    use itertools::Itertools;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::future::ready;
    use std::future::Future;
    use std::ops::ControlFlow::{Break, Continue};
    use std::pin::Pin;
    use std::rc::Rc;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::{Intrinsic, Special};
    use yash_env::io::Fd;
    use yash_env::job::Pid;
    use yash_env::semantics::Divert;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::system::Errno;
    use yash_env::variable::Scalar;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;
    use yash_env::Env;
    use yash_env::VirtualSystem;

    #[derive(Clone, Debug)]
    pub struct LocalExecutor(pub LocalSpawner);

    impl yash_env::system::r#virtual::Executor for LocalExecutor {
        fn spawn(
            &self,
            task: Pin<Box<dyn Future<Output = ()>>>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.0
                .spawn_local(task)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        }
    }

    /// Helper function to perform a test in a virtual system with an executor.
    pub fn in_virtual_system<F, Fut>(f: F)
    where
        F: FnOnce(Env, Pid, Rc<RefCell<SystemState>>) -> Fut,
        Fut: Future<Output = ()> + 'static,
    {
        let system = VirtualSystem::new();
        let pid = system.process_id;
        let state = Rc::clone(&system.state);
        let mut executor = futures_executor::LocalPool::new();
        state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));

        let env = Env::with_system(Box::new(system));
        let shared_system = env.system.clone();
        let task = f(env, pid, Rc::clone(&state));
        let done = Rc::new(Cell::new(false));
        let done_2 = Rc::clone(&done);

        executor
            .spawner()
            .spawn_local(async move {
                task.await;
                done.set(true);
            })
            .unwrap();

        while !done_2.get() {
            executor.run_until_stalled();
            shared_system.select(false).unwrap();
            SystemState::select_all(&state);
        }
    }

    fn return_builtin_main(
        _env: &mut Env,
        mut args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
        let divert = match args.get(1) {
            Some(field) if field.value == "-n" => {
                args.remove(1);
                Continue(())
            }
            _ => Break(Divert::Return),
        };
        let exit_status = match args.get(1) {
            Some(field) => field.value.parse().unwrap_or(2),
            None => 0,
        };
        Box::pin(ready((ExitStatus(exit_status), divert)))
    }

    /// Returns a minimal implementation of the `return` built-in.
    pub fn return_builtin() -> Builtin {
        Builtin {
            r#type: Special,
            execute: return_builtin_main,
        }
    }

    fn local_builtin_main(
        env: &mut Env,
        args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
        Box::pin(async move {
            for Field { value, origin } in args.into_iter().skip(1) {
                if let Some(eq_index) = value.find('=') {
                    let name = value[..eq_index].to_owned();
                    // TODO reject invalid name
                    let value = value[eq_index + 1..].to_owned();
                    let value = Variable {
                        value: Scalar(value),
                        last_assigned_location: Some(origin),
                        is_exported: false,
                        read_only_location: None,
                    };
                    if let Err(error) = env.variables.assign(Scope::Local, name, value) {
                        unimplemented!("assignment error: {:?}", error);
                    }
                } else {
                    let name = value;
                    if let Some(var) = env.variables.get(&name) {
                        if let Scalar(value) = &var.value {
                            let line = format!("{}={}\n", name, value);
                            if let Err(errno) =
                                env.system.write_all(Fd::STDOUT, line.as_bytes()).await
                            {
                                unimplemented!("write error: {:?}", errno);
                            }
                        }
                    }
                }
            }
            (ExitStatus::SUCCESS, Continue(()))
        })
    }

    pub fn local_builtin() -> Builtin {
        Builtin {
            r#type: Intrinsic,
            execute: local_builtin_main,
        }
    }

    fn echo_builtin_main(
        env: &mut Env,
        args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
        Box::pin(async move {
            let fields = (&args[1..]).iter().map(|f| &f.value).format(" ");
            let message = format!("{}\n", fields);
            let result = match env.system.write_all(Fd::STDOUT, message.as_bytes()).await {
                Ok(_) => ExitStatus::SUCCESS,
                Err(_) => ExitStatus::FAILURE,
            };
            (result, Continue(()))
        })
    }

    /// Returns a minimal implementation of the `echo` built-in.
    pub fn echo_builtin() -> Builtin {
        Builtin {
            r#type: Intrinsic,
            execute: echo_builtin_main,
        }
    }

    fn cat_builtin_main(
        env: &mut Env,
        _args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
        async fn inner(env: &mut Env) -> std::result::Result<(), Errno> {
            let mut buffer = [0; 1024];
            loop {
                let count = env.system.read_async(Fd::STDIN, &mut buffer).await?;
                if count == 0 {
                    break Ok(());
                }
                env.system.write_all(Fd::STDOUT, &buffer[..count]).await?;
            }
        }

        Box::pin(async move {
            let result = match inner(env).await {
                Ok(_) => ExitStatus::SUCCESS,
                Err(_) => ExitStatus::FAILURE,
            };
            (result, Continue(()))
        })
    }

    /// Returns a minimal implementation of the `cat` built-in.
    pub fn cat_builtin() -> Builtin {
        Builtin {
            r#type: Intrinsic,
            execute: cat_builtin_main,
        }
    }
}
