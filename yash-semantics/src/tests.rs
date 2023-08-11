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

//! Utility for unit tests

use assert_matches::assert_matches;
use futures_executor::LocalSpawner;
use futures_util::task::LocalSpawnExt;
use itertools::Itertools;
use std::cell::Cell;
use std::cell::RefCell;
use std::future::pending;
use std::future::ready;
use std::future::Future;
use std::ops::ControlFlow::Break;
use std::pin::Pin;
use std::rc::Rc;
use std::str::from_utf8;
use yash_env::builtin::Builtin;
use yash_env::builtin::Type::{Mandatory, Special};
use yash_env::io::Fd;
use yash_env::job::Pid;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::r#virtual::FileBody;
use yash_env::system::r#virtual::INode;
use yash_env::system::r#virtual::SystemState;
use yash_env::system::Errno;
use yash_env::trap::Signal;
use yash_env::variable::Scalar;
use yash_env::variable::Scope;
use yash_env::variable::Variable;
use yash_env::Env;
use yash_env::System;
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
    F: FnOnce(Env, Rc<RefCell<SystemState>>) -> Fut,
    Fut: Future<Output = ()> + 'static,
{
    let system = VirtualSystem::new();
    let state = Rc::clone(&system.state);
    let mut executor = futures_executor::LocalPool::new();
    state.borrow_mut().executor = Some(Rc::new(LocalExecutor(executor.spawner())));

    let env = Env::with_system(Box::new(system));
    let shared_system = env.system.clone();
    let task = f(env, Rc::clone(&state));
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

pub fn stub_tty(state: &RefCell<SystemState>) {
    state
        .borrow_mut()
        .file_system
        .save("/dev/tty", Rc::new(RefCell::new(INode::new([]))))
        .unwrap();
}

/// Helper function for asserting on the content of /dev/stdout.
pub fn assert_stdout<F, T>(state: &RefCell<SystemState>, f: F) -> T
where
    F: FnOnce(&str) -> T,
{
    let stdout = state.borrow().file_system.get("/dev/stdout").unwrap();
    let stdout = stdout.borrow();
    assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
        f(from_utf8(content).unwrap())
    })
}

/// Helper function for asserting on the content of /dev/stderr.
pub fn assert_stderr<F, T>(state: &RefCell<SystemState>, f: F) -> T
where
    F: FnOnce(&str) -> T,
{
    let stderr = state.borrow().file_system.get("/dev/stderr").unwrap();
    let stderr = stderr.borrow();
    assert_matches!(&stderr.body, FileBody::Regular { content, .. } => {
        f(from_utf8(content).unwrap())
    })
}

fn exit_builtin_main(
    env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let exit_status = args
        .get(0)
        .map(|field| ExitStatus(field.value.parse().unwrap_or(2)));
    let result = yash_env::builtin::Result::with_exit_status_and_divert(
        env.exit_status,
        Break(Divert::Exit(exit_status)),
    );
    Box::pin(ready(result))
}

/// Returns a minimal implementation of the `exit` built-in.
pub fn exit_builtin() -> Builtin {
    Builtin {
        r#type: Special,
        execute: exit_builtin_main,
    }
}

fn return_builtin_main(
    env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let mut i = args.iter().peekable();
    let no_return = i.next_if(|field| field.value == "-n").is_some();
    let exit_status = i.next().map(|arg| ExitStatus(arg.value.parse().unwrap()));
    let result = if no_return {
        yash_env::builtin::Result::new(exit_status.unwrap_or(env.exit_status))
    } else {
        yash_env::builtin::Result::with_exit_status_and_divert(
            env.exit_status,
            Break(Divert::Return(exit_status)),
        )
    };
    Box::pin(ready(result))
}

/// Returns a minimal implementation of the `return` built-in.
pub fn return_builtin() -> Builtin {
    Builtin {
        r#type: Special,
        execute: return_builtin_main,
    }
}

fn break_builtin_main(
    _env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let count = args.get(0).map_or(1, |field| field.value.parse().unwrap());
    let result = yash_env::builtin::Result::with_exit_status_and_divert(
        ExitStatus::SUCCESS,
        Break(Divert::Break { count: count - 1 }),
    );
    Box::pin(ready(result))
}

/// Returns a minimal implementation of the `break` built-in.
pub fn break_builtin() -> Builtin {
    Builtin {
        r#type: Special,
        execute: break_builtin_main,
    }
}

fn continue_builtin_main(
    _env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let count = args.get(0).map_or(1, |field| field.value.parse().unwrap());
    let result = yash_env::builtin::Result::with_exit_status_and_divert(
        ExitStatus::SUCCESS,
        Break(Divert::Continue { count: count - 1 }),
    );
    Box::pin(ready(result))
}

/// Returns a minimal implementation of the `continue` built-in.
pub fn continue_builtin() -> Builtin {
    Builtin {
        r#type: Special,
        execute: continue_builtin_main,
    }
}

fn suspend_builtin_main(
    env: &mut Env,
    _args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
    env.system
        .kill(Pid::from_raw(0), Some(Signal::SIGSTOP))
        .unwrap();
    Box::pin(pending())
}

/// Returns a minimal implementation of the `suspend` built-in.
pub fn suspend_builtin() -> Builtin {
    Builtin {
        r#type: Special,
        execute: suspend_builtin_main,
    }
}

fn local_builtin_main(
    env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
    Box::pin(async move {
        for Field { value, origin } in args {
            if let Some(eq_index) = value.find('=') {
                let name = value[..eq_index].to_owned();
                // TODO reject invalid name
                let value = value[eq_index + 1..].to_owned();
                let value = Variable::new(value).set_assigned_location(origin);
                if let Err(error) = env.variables.assign(Scope::Local, name, value) {
                    unimplemented!("assignment error: {:?}", error);
                }
            } else {
                let name = value;
                if let Some(var) = env.variables.get(&name) {
                    if let Some(Scalar(value)) = &var.value {
                        let line = format!("{name}={value}\n");
                        if let Err(errno) = env.system.write_all(Fd::STDOUT, line.as_bytes()).await
                        {
                            unimplemented!("write error: {:?}", errno);
                        }
                    }
                }
            }
        }
        Default::default()
    })
}

pub fn local_builtin() -> Builtin {
    Builtin {
        r#type: Mandatory,
        execute: local_builtin_main,
    }
}

fn echo_builtin_main(
    env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
    Box::pin(async move {
        let fields = args.iter().map(|f| &f.value).format(" ");
        let message = format!("{fields}\n");
        let result = match env.system.write_all(Fd::STDOUT, message.as_bytes()).await {
            Ok(_) => ExitStatus::SUCCESS,
            Err(_) => ExitStatus::FAILURE,
        };
        result.into()
    })
}

/// Returns a minimal implementation of the `echo` built-in.
pub fn echo_builtin() -> Builtin {
    Builtin {
        r#type: Mandatory,
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
        result.into()
    })
}

/// Returns a minimal implementation of the `cat` built-in.
pub fn cat_builtin() -> Builtin {
    Builtin {
        r#type: Mandatory,
        execute: cat_builtin_main,
    }
}
