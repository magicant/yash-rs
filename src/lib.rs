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

pub mod parser;
pub mod source;
pub mod syntax;

// TODO Allow user to select input source
// TODO Execute the command after parsing
async fn parse_and_print() {
    let input = std::io::stdin();
    loop {
        let mut code = String::new();
        if input
            .read_line(&mut code)
            .expect("input should be readable")
            == 0
        {
            break;
        }
        let mut parser = parser::Parser::new(code);
        match parser.parse_simple_command().await {
            Ok(command) => println!("{}", command),
            Err(e) => print!("{}", e),
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
