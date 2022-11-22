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

pub use yash_arith as arith;
pub use yash_builtin as builtin;
pub use yash_env as env;
pub use yash_fnmatch as fnmatch;
pub use yash_quote as quote;
pub use yash_semantics as semantics;
#[doc(no_inline)]
pub use yash_syntax::{alias, parser, source, syntax};

// TODO Allow user to select input source
async fn parse_and_print(mut env: yash_env::Env) -> i32 {
    use env::option::State::Off;
    use std::cell::Cell;
    use std::num::NonZeroU64;
    use std::ops::ControlFlow::Break;
    use std::rc::Rc;
    use yash_env::input::Stdin;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;

    let mut args = std::env::args();
    if let Some(arg0) = args.next() {
        env.arg0 = arg0;
    }

    env.builtins.extend(builtin::BUILTINS.iter().cloned());

    // TODO std::env::vars() would panic on broken UTF-8, which should rather be
    // ignored.
    for (name, value) in std::env::vars() {
        let value = Variable::new(value).export();
        env.variables.assign(Scope::Global, name, value).unwrap();
    }
    // TODO Ignore unsafe variables such as $IFS

    let mut input = Box::new(Stdin::new(env.system.clone()));
    let echo = Rc::new(Cell::new(Off));
    input.set_echo(Some(Rc::clone(&echo)));
    let line = NonZeroU64::new(1).unwrap();
    let mut lexer = parser::lex::Lexer::new(input, line, source::Source::Stdin);
    let mut rel = semantics::ReadEvalLoop::new(&mut env, &mut lexer);
    rel.set_verbose(Some(echo));
    let result = rel.run().await;
    if let Break(divert) = result {
        if let Some(exit_status) = divert.exit_status() {
            env.exit_status = exit_status;
        }
    }
    env.exit_status.0
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
        system.select(false).ok();
    }
}
