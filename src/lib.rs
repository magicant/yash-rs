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

pub mod input;
pub mod parser;
pub mod source;
pub mod syntax;

// TODO Allow user to select input source
// TODO Execute the command after parsing
async fn parse_and_print() {
    use std::future::ready;
    use std::future::Future;
    use std::num::NonZeroU64;
    use std::pin::Pin;
    use std::rc::Rc;

    struct Stdin;

    impl input::Input for Stdin {
        fn next_line(
            &mut self,
            _: &input::Context,
        ) -> Pin<Box<dyn Future<Output = Result<Rc<source::Line>, input::Error>>>> {
            Box::pin(ready({
                let mut code = String::new();
                std::io::stdin()
                    .read_line(&mut code)
                    .map(|_|
                    // TODO correct line number
                    Rc::new(source::Line {
                        value:code,
                        number:NonZeroU64::new(1).unwrap(),
                        source:source::Source::Unknown,
                    }))
                    .map_err(|e| (source::Location::dummy("".to_string()), e))
            }))
        }
    }

    loop {
        let mut lexer = parser::lex::Lexer::new(Box::new(Stdin));
        let mut parser = parser::Parser::new(&mut lexer);
        match parser.command_line().await {
            Ok(None) => break,
            Ok(Some(command)) => println!("{}", command),
            Err(e) => println!("{}", e),
        }
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
