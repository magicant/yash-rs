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
pub mod syntax;

// TODO Read input from stdin or file
// TODO Execute the command after parsing
async fn parse_and_print() {
    let mut parser = parser::Parser::new("echo hello world".to_string());
    println!("{}", parser.parse_command());
}

pub fn bin_main() {
    let mut pool = futures::executor::LocalPool::new();
    use futures::task::SpawnExt;
    pool.spawner()
        .spawn(parse_and_print())
        .expect("spawn should succeed");
    pool.run();
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
