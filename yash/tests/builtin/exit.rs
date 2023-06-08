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

use crate::{file_with_content, subject};
use std::str::from_utf8;

#[test]
fn specified_exit_status() {
    let stdin = file_with_content(b"exit 12\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(12), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn default_exit_status_without_trap() {
    let stdin = file_with_content(b"(exit 5); exit\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(5), "{:?}", result.status);
}

#[test]
fn specified_exit_status_in_signal_trap() {
    let stdin = file_with_content(b"trap '(exit 2); exit 3' INT; (exit 1); kill -INT $$\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(3), "{:?}", result.status);
}

#[test]
fn default_exit_status_in_signal_trap() {
    let stdin = file_with_content(b"trap '(exit 2); exit' INT; (exit 1); kill -INT $$\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{:?}", result.status);
}
