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

//! TODO Elaborate

pub use yash_builtin as builtin;
pub use yash_env as env;
pub use yash_semantics as semantics;
#[doc(no_inline)]
pub use yash_syntax::{alias, parser, source, syntax};

// TODO Allow user to select input source
async fn parse_and_print(mut env: yash_env::Env) -> i32 {
    use semantics::Command;
    use semantics::Handle;
    use std::ops::ControlFlow::{Break, Continue};
    use yash_env::input::Stdin;
    use yash_env::variable::Scope;
    use yash_env::variable::Value::Scalar;
    use yash_env::variable::Variable;

    env.builtins.extend(builtin::BUILTINS.iter().cloned());
    // TODO std::env::vars() would panic on broken UTF-8, which should rather be
    // ignored.
    for (name, value) in std::env::vars() {
        let value = Variable {
            value: Scalar(value),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        };
        env.variables.assign(Scope::Global, name, value).unwrap();
    }

    let mut lexer = parser::lex::Lexer::new(Box::new(Stdin::new(env.system.clone())));
    loop {
        let mut parser = parser::Parser::with_aliases(&mut lexer, env.aliases.clone());
        match parser.command_line().await {
            Ok(None) => break env.exit_status.0,
            Ok(Some(command)) => match command.execute(&mut env).await {
                Continue(()) => (),
                // TODO Handle divert
                Break(divert) => env.print_error(&format_args!("{:?}", divert)).await,
            },
            Err(e) => {
                match e.handle(&mut env).await {
                    Continue(()) => (),
                    // TODO Handle divert
                    Break(divert) => env.print_error(&format_args!("{:?}", divert)).await,
                }

                lexer.reset();
            }
        }
        // TODO If the lexer still has unconsumed input, it should be parsed
        // before the lexer is dropped.
    }
}

pub fn bin_main() -> i32 {
    use env::Env;
    use env::RealSystem;
    use futures_util::task::LocalSpawnExt;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::task::Poll;

    // SAFETY: This is the only instance of RealSystem we create in the whole
    // process.
    let system = unsafe { RealSystem::new() };
    let env = Env::with_system(Box::new(system));
    let system = env.system.clone();
    let mut pool = futures_executor::LocalPool::new();
    let task = parse_and_print(env);
    let result = Rc::new(Cell::new(Poll::Pending));
    let result_2 = Rc::clone(&result);
    pool.spawner()
        .spawn_local(async move {
            let result = task.await;
            result_2.set(Poll::Ready(result));
        })
        .unwrap();

    loop {
        pool.run_until_stalled();
        match result.get() {
            Poll::Ready(result) => return result,
            Poll::Pending => (),
        }
        system.select().ok();
    }
}
