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
fn breaking_one_for_loop_not_nested() {
    let stdin = file_with_content(
        b"for i in 1 2 3; do
            echo in $i
            break 1
            echo out $i
        done
        echo done $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok("in 1\ndone 0\n"));
}

#[test]
fn breaking_one_while_loop_not_nested() {
    let stdin = file_with_content(
        b"while true; do
            echo in
            break 1
            echo out
        done
        echo done $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok("in\ndone 0\n"));
}

#[test]
fn breaking_one_for_loop_nested_in_for_loop() {
    let stdin = file_with_content(
        b"for i in 1 2 3; do
            echo in $i
            for j in a b c; do
                echo in $i $j
                break 1
                echo out $i $j
            done
            echo out $i
        done
        echo done $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(
        from_utf8(&result.stdout),
        Ok("in 1
in 1 a
out 1
in 2
in 2 a
out 2
in 3
in 3 a
out 3
done 0
")
    );
}

#[test]
fn breaking_one_while_loop_nested_in_while_loop() {
    let stdin = file_with_content(
        b"i=1
        while [ $i -le 3 ]; do
            echo in outer $i
            while true; do
                echo in inner $i
                break 1
                echo out inner $i
            done
            echo out outer $i
            i=$((i+1))
        done
        echo done $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(
        from_utf8(&result.stdout),
        Ok("in outer 1
in inner 1
out outer 1
in outer 2
in inner 2
out outer 2
in outer 3
in inner 3
out outer 3
done 0
")
    );
}

#[test]
fn breaking_two_for_loops_outermost() {
    let stdin = file_with_content(
        b"for i in 1 2 3; do
            echo in $i
            for j in a b c; do
                echo in $i $j
                break 2
                echo out $i $j
            done
            echo out $i
        done
        echo done $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok("in 1\nin 1 a\ndone 0\n"));
}

#[test]
fn default_operand_is_1() {
    let stdin = file_with_content(
        b"for i in 1; do
            echo in $i
            for j in a; do
                echo in $i $j
                for k in +; do
                    echo in $i $j $k
                    break
                    echo out $i $j $k
                done
                echo out $i $j
            done
            echo out $i
        done
        echo done $?\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(
        from_utf8(&result.stdout),
        Ok("in 1
in 1 a
in 1 a +
out 1 a
out 1
done 0
")
    );
}

#[test]
fn exit_status_of_break_after_failed_command() {
    let stdin = file_with_content(b"for i in 1; do false; break; done\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(from_utf8(&result.stdout), Ok(""));
}

#[test]
fn operand_zero() {
    let stdin = file_with_content(b"for i in 1; do break 0; done\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_ne!(result.status.code(), Some(0));
    assert_ne!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_one_more_than_actual_nest_level_1() {
    let stdin = file_with_content(b"for i in 1; do break 2; echo not reached; done\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_one_more_than_actual_nest_level_2() {
    let stdin = file_with_content(
        b"for i in 1; do
            for j in a; do
                break 3
                echo not reached 1
            done
            echo not reached 2
        done\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

// This is a questionable case. Is this really a "lexically enclosing" loop as
// defined in POSIX? Most shells (other than mksh) support this case.
// TODO breaking_out_of_eval

#[test]
fn negated_break() {
    let stdin = file_with_content(b"for i in 1; do ! break; echo not reached; done\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_before_and_then() {
    let stdin = file_with_content(
        b"for i in 1; do break && echo not reached 1; echo not reached 2; done\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_after_and_then() {
    let stdin = file_with_content(b"for i in 1; do true && break; echo not reached $?; done\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_before_or_else() {
    let stdin = file_with_content(
        b"for i in 1; do break || echo not reached 1; echo not reached 2; done\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_after_or_else() {
    let stdin = file_with_content(b"for i in 1; do false || break; echo not reached $?; done\n");
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok(""));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}

#[test]
fn breaking_out_of_other_compound_commands() {
    let stdin = file_with_content(
        b"for i in 1; do
            { 
                case a in (a)
                    if break; echo not reached 1; then echo not reached 2; fi
                    echo not reached 3
                esac
                echo not reached 4
            }
            echo not reached 5
        done
        echo reached\n",
    );
    let result = subject().stdin(stdin).output().unwrap();
    assert_eq!(from_utf8(&result.stdout), Ok("reached\n"));
    assert_eq!(from_utf8(&result.stderr), Ok(""));
}
