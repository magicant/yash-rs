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
pub use yash_env::env;
pub use yash_semantics as semantics;
pub use yash_syntax::*;

// TODO Allow user to select input source
async fn parse_and_print() {
    use env::Env;
    use semantics::Command;
    use std::num::NonZeroU64;

    struct Stdin;

    #[async_trait::async_trait(?Send)]
    impl input::Input for Stdin {
        async fn next_line(&mut self, _: &input::Context) -> input::Result {
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
        }
    }

    let mut env = Env {
        aliases: Default::default(),
        builtins: builtin::BUILTINS.iter().copied().collect(),
        functions: Default::default(),
        variables: Default::default(),
    };

    loop {
        let mut lexer = parser::lex::Lexer::new(Box::new(Stdin));
        let mut parser = parser::Parser::with_aliases(&mut lexer, env.aliases.clone());
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

pub fn bin_main() {
    let mut pool = futures::executor::LocalPool::new();
    use futures::task::LocalSpawnExt;
    pool.spawner()
        .spawn_local(parse_and_print())
        .expect("spawn should succeed");
    pool.run();
}
