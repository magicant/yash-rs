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

use itertools::Itertools;
use std::future::ready;
use std::future::Future;
use std::ops::ControlFlow::Break;
use std::pin::Pin;
use yash_env::builtin::Builtin;
use yash_env::builtin::Type::{Mandatory, Special};
use yash_env::io::Fd;
use yash_env::job::Pid;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::r#virtual::SIGSTOP;
use yash_env::system::Errno;
use yash_env::variable::Scope;
use yash_env::Env;
use yash_env::System;

fn exit_builtin_main(
    env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let exit_status = args
        .first()
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
        is_declaration_utility: Some(false),
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
        is_declaration_utility: Some(false),
    }
}

fn break_builtin_main(
    _env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let count = args.first().map_or(1, |field| field.value.parse().unwrap());
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
        is_declaration_utility: Some(false),
    }
}

fn continue_builtin_main(
    _env: &mut Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result>>> {
    let count = args.first().map_or(1, |field| field.value.parse().unwrap());
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
        is_declaration_utility: Some(false),
    }
}

fn suspend_builtin_main(
    env: &mut Env,
    _args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
    Box::pin(async move {
        env.system.kill(Pid(0), Some(SIGSTOP)).await.unwrap();
        yash_env::builtin::Result::default()
    })
}

/// Returns a minimal implementation of the `suspend` built-in.
pub fn suspend_builtin() -> Builtin {
    Builtin {
        r#type: Special,
        execute: suspend_builtin_main,
        is_declaration_utility: Some(false),
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
                let mut var = env.variables.get_or_new(name, Scope::Local);
                if let Err(error) = var.assign(value, origin) {
                    unimplemented!("assignment error: {:?}", error);
                }
            } else {
                let name = value;
                if let Some(var) = env.variables.get(&name) {
                    if let Some(value) = &var.value {
                        let line = format!("{name}={}\n", value.quote());
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
        is_declaration_utility: Some(true),
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
        is_declaration_utility: Some(false),
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
        is_declaration_utility: Some(false),
    }
}
