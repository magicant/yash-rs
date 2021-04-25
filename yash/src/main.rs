// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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
pub use yash_core::alias;
pub use yash_core::env;
pub use yash_core::source;
pub use yash_syntax::exec;
pub use yash_syntax::expansion;
pub use yash_syntax::input;
pub use yash_syntax::parser;
pub use yash_syntax::syntax;

// TODO Allow user to select input source
async fn parse_and_print() {
    use crate::env::AliasEnv;
    use crate::env::NativeEnv;
    use std::future::ready;
    use std::future::Future;
    use std::num::NonZeroU64;
    use std::pin::Pin;

    struct Stdin;

    impl input::Input for Stdin {
        fn next_line(
            &mut self,
            _: &input::Context,
        ) -> Pin<Box<dyn Future<Output = crate::input::Result>>> {
            Box::pin(ready({
                let mut code = String::new();
                std::io::stdin()
                    .read_line(&mut code)
                    .map(|_| source::Line {
                        value: code,
                        // TODO correct line number
                        number: NonZeroU64::new(1).unwrap(),
                        source: source::Source::Unknown,
                    })
                    .map_err(|e| (source::Location::dummy("".to_string()), e))
            }))
        }
    }

    let mut env = NativeEnv::new();
    let builtins = &mut env.local.builtins.0;
    builtins.extend(builtin::BUILTINS.iter().copied());

    loop {
        let mut lexer = parser::lex::Lexer::new(Box::new(Stdin));
        let mut parser = parser::Parser::with_aliases(&mut lexer, env.aliases().clone());
        match parser.command_line().await {
            Ok(None) => break,
            Ok(Some(command)) => command
                .execute(&mut env)
                .await
                .unwrap_or_else(|a| eprintln!("{:?}", a)),
            Err(e) => println!("{}", e),
        }
        // TODO If the lexer still has unconsumed input, it should be parsed
        // before the lexer is dropped.
    }
}

fn main() {
    let mut pool = futures::executor::LocalPool::new();
    use futures::task::LocalSpawnExt;
    pool.spawner()
        .spawn_local(parse_and_print())
        .expect("spawn should succeed");
    pool.run();
}
