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

mod builtin;

const BIN: &str = env!("CARGO_BIN_EXE_yash");

fn subject() -> std::process::Command {
    use std::env::var_os;
    use std::process::Command;

    let mut command = Command::new(BIN);
    command.env_clear();
    command.env("PATH", var_os("PATH").unwrap());
    command
}

fn file_with_content(content: &[u8]) -> std::fs::File {
    use std::io::Seek;
    use std::io::Write;
    use tempfile::tempfile;

    let mut stdin = tempfile().unwrap();
    stdin.write_all(content).unwrap();
    stdin.rewind().unwrap();
    stdin
}
