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

//! Our scripted tests are performed by the `run-test.sh` script that runs the
//! test subject with its standard input redirected to a prepared file and then
//! examines the results. Test cases are written in script files named with the
//! `-p.sh` or `-y.sh` suffix.

use std::path::Path;
use std::process::Command;
use std::process::Stdio;

const BIN: &str = env!("CARGO_BIN_EXE_yash");
const TMPDIR: &str = env!("CARGO_TARGET_TMPDIR");

fn run(name: &str) {
    // TODO Reset signal blocking mask

    let mut log_file = Path::new(TMPDIR).join(name);
    log_file.set_extension("log");

    let result = Command::new("sh")
        .env("TMPDIR", TMPDIR)
        .current_dir("tests/scripted_test")
        .stdin(Stdio::null())
        .arg("./run-test.sh")
        .arg(BIN)
        .arg(name)
        .arg(&log_file)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);

    // The `run-test.sh` script returns a successful exit status even if there
    // is a failed test case. Check the log file to see if there is one.

    let log = std::fs::read_to_string(&log_file).unwrap();
    assert!(!log.contains("FAILED"), "{}", log);
}

#[test]
fn and_or_list() {
    run("andor-p.sh")
}

#[test]
fn arithmetic_expansion() {
    run("arith-p.sh")
}

#[test]
fn asynchronous_list() {
    run("async-p.sh")
}

#[test]
fn break_builtin() {
    run("break-p.sh")
}

#[test]
fn case_command() {
    run("case-p.sh")
}

#[test]
fn cd_builtin() {
    run("cd-p.sh")
}

#[test]
fn command_substitution() {
    run("cmdsub-p.sh")
}

#[test]
fn comment() {
    run("comment-p.sh")
}

#[test]
fn continue_builtin() {
    run("continue-p.sh")
}

#[test]
fn exec_builtin() {
    run("exec-p.sh")
}

#[test]
fn exit_builtin() {
    run("exit-p.sh")
}

#[test]
fn fnmatch() {
    run("fnmatch-p.sh")
}

#[test]
fn field_splitting() {
    run("fsplit-p.sh")
}

#[test]
fn function() {
    run("function-p.sh")
}

#[test]
fn grouping() {
    run("grouping-p.sh")
}

#[test]
fn if_command() {
    run("if-p.sh")
}

// a.k.a. globbing
#[test]
fn pathname_expansion() {
    run("path-p.sh")
}

#[test]
fn pipeline() {
    run("pipeline-p.sh")
}

#[test]
fn ppid_variable() {
    run("ppid-p.sh")
}

#[test]
fn quotation() {
    run("quote-p.sh")
}

#[test]
fn redirection() {
    run("redir-p.sh")
}

#[test]
fn return_builtin() {
    run("return-p.sh")
}

#[test]
fn simple_command() {
    run("simple-p.sh")
}

#[test]
fn startup() {
    run("startup-p.sh")
}

#[test]
fn tilde_expansion() {
    run("tilde-p.sh")
}

#[test]
fn until_loop() {
    run("until-p.sh")
}

#[test]
fn while_loop() {
    run("while-p.sh")
}
