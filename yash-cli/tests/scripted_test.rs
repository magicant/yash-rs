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

use pty::run_with_pty;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

mod pty;

const BIN: &str = env!("CARGO_BIN_EXE_yash3");
const TMPDIR: &str = env!("CARGO_TARGET_TMPDIR");

/// Runs a test subject.
///
/// You would normally not use this function directly. Instead, use one of the
/// [`run`] or [`run_with_pty`] functions.
unsafe fn run_with_preexec<F>(name: &str, pre_exec: F)
where
    F: FnMut() -> std::io::Result<()> + Send + Sync + 'static,
{
    // TODO Reset signal blocking mask

    let mut log_file = Path::new(TMPDIR).join(name);
    log_file.set_extension("log");

    let mut command = Command::new("sh");
    command
        .env("TMPDIR", TMPDIR)
        .current_dir("tests/scripted_test")
        .stdin(Stdio::null())
        .arg("./run-test.sh")
        .arg(BIN)
        .arg(name)
        .arg(&log_file);
    unsafe {
        command.pre_exec(pre_exec);
    }
    let result = command.output().unwrap();
    assert!(result.status.success(), "{:?}", result);

    // The `run-test.sh` script returns a successful exit status even if there
    // is a failed test case. Check the log file to see if there is one.

    let log = std::fs::read_to_string(&log_file).unwrap();
    let failures = failures(&log);
    assert!(failures.is_empty(), "{failures}");
}

/// Runs a test subject.
///
/// This function runs the test subject in the current session. To run it in a
/// separate session, use [`run_with_pty`].
fn run(name: &str) {
    unsafe { run_with_preexec(name, || Ok(())) }
}

/// Extracts the failed test cases from the log file.
fn failures(log: &str) -> String {
    let mut lines = log.lines();
    let mut test_case = Vec::new();
    let mut result = String::new();

    // Each test case in the log file is enclosed by the "%%% START: " and
    // "%%% PASSED: " or "%%% FAILED: " lines. We extract lines between these
    // markers and append them to the result string.
    while let Some(start) = lines.find(|line| line.starts_with("%%% START: ")) {
        test_case.clear();
        test_case.push(start);
        for line in lines.by_ref() {
            if line.starts_with("%%% PASSED: ") {
                // Discard this test case
                break;
            } else if line.starts_with("%%% FAILED: ") {
                test_case.push(line);

                // Add this test case to the result
                for line in test_case.drain(..) {
                    result.push_str(line);
                    result.push('\n');
                }
                result.push('\n');

                break;
            } else {
                test_case.push(line);
            }
        }
    }

    result
}

#[test]
fn alias() {
    run("alias-p.sh")
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
fn bg_builtin() {
    run_with_pty("bg-p.sh")
}

#[test]
fn break_builtin() {
    run("break-p.sh")
}

#[test]
fn builtins() {
    run("builtins-p.sh")
}

#[test]
fn case_command() {
    run("case-p.sh")
}

#[test]
fn case_command_ex() {
    run("case-y.sh");
}

#[test]
fn cd_builtin() {
    run("cd-p.sh")
}

#[test]
fn command_builtin() {
    run("command-p.sh")
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
fn errexit_option() {
    run("errexit-p.sh")
}

#[test]
fn error_consequences() {
    run("error-p.sh")
}

#[test]
fn error_consequences_ex() {
    run("error-y.sh")
}

#[test]
fn eval_builtin() {
    run("eval-p.sh")
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
fn export_builtin() {
    run("export-p.sh")
}

#[test]
fn false_builtin() {
    run("false-p.sh")
}

#[test]
fn fg_builtin() {
    run_with_pty("fg-p.sh")
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
fn for_loop() {
    run("for-p.sh")
}

#[test]
fn function() {
    run("function-p.sh")
}

#[test]
fn getopts_builtin() {
    run("getopts-p.sh")
}

#[test]
fn grouping() {
    run("grouping-p.sh")
}

#[test]
fn if_command() {
    run("if-p.sh")
}

#[test]
fn input() {
    run("input-p.sh")
}

#[test]
fn job_control() {
    run_with_pty("job-p.sh")
}

#[test]
fn job_control_ex() {
    run_with_pty("job-y.sh")
}

#[test]
fn kill_builtin_1() {
    run("kill1-p.sh")
}

#[test]
fn kill_builtin_2() {
    run("kill2-p.sh")
}

#[test]
fn kill_builtin_3() {
    run("kill3-p.sh")
}

#[test]
fn kill_builtin_4() {
    run_with_pty("kill4-p.sh")
}

#[test]
fn lineno() {
    run("lineno-p.sh")
}

#[test]
fn nop_builtins() {
    run("nop-p.sh")
}

#[test]
fn options() {
    run("option-p.sh")
}

#[test]
fn options_ex() {
    run("option-y.sh")
}

#[test]
fn parameter_expansion() {
    run("param-p.sh")
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
fn read_builtin() {
    run("read-p.sh")
}

#[test]
fn readonly_builtin() {
    run("readonly-p.sh")
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
fn set_builtin() {
    run("set-p.sh")
}

#[test]
fn shift_builtin() {
    run("shift-p.sh")
}

#[test]
fn simple_command() {
    run("simple-p.sh")
}

#[test]
fn source_builtin() {
    run("source-p.sh")
}

#[test]
fn startup() {
    run("startup-p.sh")
}

#[test]
fn startup_ex() {
    run("startup-y.sh")
}

#[test]
fn tilde_expansion() {
    run("tilde-p.sh")
}

// This test case also covers the behavior of the trap execution.
#[test]
fn trap_builtin() {
    run("trap-p.sh")
}

#[test]
fn trap_ex_2() {
    run_with_pty("trap2-y.sh")
}

#[test]
fn true_builtin() {
    run("true-p.sh")
}

#[test]
fn typeset_builtin() {
    run("typeset-y.sh")
}

#[test]
fn ulimit_builtin() {
    run("ulimit-y.sh")
}

#[test]
fn umask_builtin() {
    run("umask-p.sh")
}

#[test]
fn unset_builtin() {
    run("unset-p.sh")
}

#[test]
fn until_loop() {
    run("until-p.sh")
}

#[test]
fn wait_builtin() {
    run_with_pty("wait-p.sh")
}

#[test]
fn while_loop() {
    run("while-p.sh")
}
