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
fn exiting_from_function_without_exit_status() {
    let stdin = file_with_content(b"f() { (exit 47); return; echo X; }\nf\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(47), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn exiting_from_function_with_exit_status() {
    let stdin = file_with_content(b"f() { return 21; echo X; }\nf\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(21), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn exiting_from_nested_function() {
    let stdin = file_with_content(b"f() { return 93; echo X $?; }\ng() { f; echo Y $?; }\ng\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok("Y 93\n"));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn exiting_from_signal_trap_running_function_without_exit_status() {
    let stdin = file_with_content(
        b"trap '(exit 1); return; echo X $?' INT
        f() { (kill -INT $$; exit 2); echo Y $?; }
        f
        echo Z $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok("Z 2\n"));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn exiting_from_signal_trap_running_function_with_exit_status() {
    let stdin = file_with_content(
        b"trap '(exit 1); return 10; echo X $?' INT
        f() { (kill -INT $$; exit 2); echo Y $?; }
        f
        echo Z $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok("Z 10\n"));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn exiting_from_exit_trap_running_function_without_exit_status() {
    let stdin = file_with_content(b"trap '(exit 1); return; echo X $?' EXIT\nexit 2\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(2), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn exiting_from_exit_trap_running_function_with_exit_status() {
    let stdin = file_with_content(b"trap '(exit 1); return 7; echo X $?' EXIT\nexit 3\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(7), "{:?}", result.status);
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}
