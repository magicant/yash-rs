---
name: scripted-tests
description: 'Write scripted tests for the yash shell executable: file naming, the test_* harness aliases, exit status and output checks, skipping unsupported environments, the environment a test case runs in, and cleaning up background jobs so no process is left behind. Use when shell-observable behavior changes and a test under yash-cli/tests/scripted_test is added, updated, or reviewed.'
argument-hint: 'Which shell behavior needs a scripted test?'
---

# Scripted Tests

Scripted tests run the built shell executable and compare what it produces
against an expectation declared in the test file. Write them as follows unless
you are instructed otherwise.

The harness is [run-test.sh](../../../yash-cli/tests/scripted_test/run-test.sh),
driven by [scripted_test.rs](../../../yash-cli/tests/scripted_test.rs). Read
`run-test.sh` when you need a detail this skill does not cover.

## When to write one

- Add or update a scripted test whenever shell-observable behavior changes.
  Unit tests cover the implementation; a scripted test covers what a user of the
  shell executable can see.
- Test through the shell's own interface: the exit status, the standard output,
  and the standard error of a command line. Anything only reachable from Rust
  belongs in a unit test instead.

## Choosing the file

- Test files live in
  [yash-cli/tests/scripted_test](../../../yash-cli/tests/scripted_test/) and are
  named after the feature they cover.
- A `-p.sh` file tests behavior any POSIX-compliant shell must have. It sets
  `posix="true"` near the top, which makes the harness invoke the testee as
  `sh`. Do not put yash extensions in these files.
- A `-y.sh` file tests yash-specific behavior, including behavior POSIX leaves
  unspecified and options yash adds.
- Add to the existing file for the feature. A new file needs a corresponding
  `#[test]` function in `scripted_test.rs`; use `run_with_pty` rather than `run`
  if the test needs a controlling terminal, as job control and interactive tests
  do.

## Anatomy of a test case

A test case is one `test_*` alias, a name, optional harness options, optional
arguments for the shell, and the here-documents that follow:

```sh
test_oE -e 0 'name of the test case' -o portable
echo hello
__IN__
hello
__OUT__
```

- The `__IN__` document is the shell's standard input. The operands after the
  name are passed to the shell as command-line arguments (`-i`, `-m`,
  `-o portable`, …).
- The alias says which streams are checked. A lowercase letter means the stream
  is compared against a here-document, an uppercase letter means it must be
  empty, and an absent letter means it is not checked at all:

  | Alias | Standard output | Standard error |
  |---|---|---|
  | `test_x` | not checked | not checked |
  | `test_o` | `__OUT__` | not checked |
  | `test_O` | must be empty | not checked |
  | `test_e` | not checked | `__ERR__` |
  | `test_E` | not checked | must be empty |
  | `test_oe` | `__OUT__` | `__ERR__` |
  | `test_oE` | `__OUT__` | must be empty |
  | `test_Oe` | must be empty | `__ERR__` |
  | `test_OE` | must be empty | must be empty |

- The `-d` option requires the standard error to be non-empty without comparing
  it. Use it for a case that must print a diagnostic whose exact wording is not
  the point; it overrides the alias's standard error check.
- The `-f` option inverts the result, for a case that documents an unfixed bug
  or an unimplemented feature.

## Checking the exit status

- Check an exit status with the `-e` option, not by printing `$?` from inside
  the test case. `-e` applies to the test case as a whole, so put the command
  whose status matters last.
- If the case has to run further commands after the one being tested, give the
  status its own test case rather than echoing `$?`. A test case is cheap; a
  `$?` in the middle of a case turns a status check into an output check and
  hides which command the status came from.
- `-e n` expects any non-zero status. `-e TERM` and other signal names expect
  the shell to be killed by that signal.

## Letting the harness compare

- Prefer letting the harness compare the result: print what the test produces
  and declare the expectation in a `__OUT__`/`__ERR__` section rather than
  asserting inside the test case with `[ ... ]` and `test_x`. A harness
  comparison shows the expected and the actual output when it fails, whereas an
  in-case assertion only reports a non-zero exit status.
- Assert inside the case only where the harness cannot express the check.
- Where a value needs normalizing before it can be compared, print the
  normalized value and let the expectation carry it. Keep the raw value in the
  fallback branch so a failure still shows what was actually there:

  ```sh
  case $state in
      T*) echo "job is stopped";;
      *) echo "job state is $state";;
  esac
  ```

## The environment a test case runs in

- Each test file runs in a fresh empty working directory, so a test case may
  create files freely.
- To run the shell under test from within a test case, use the `TESTEE`
  environment variable, which the harness exports with the path to the
  executable. The working directory also contains a symbolic link named `sh`
  that points at the testee, but it is what the harness itself uses to invoke
  the testee as `sh`; a test case that runs plain `sh` finds whatever the
  current `PATH` resolves, which is normally the system shell.
- The `setup` function adds a script that runs before every test case in the
  file. `setup -d` adds the default helpers (`echoraw`, `bracket`, `_sp`,
  `_tab`, `_nl`). Call `macos_kill_workaround` before test cases that rely on a
  self-sent signal being handled at a predictable time.
- The test file itself is read by the shell that drives the harness, not by the
  testee. Everything outside the here-documents — `skip` probes, `case`,
  subshells, helper functions — must be portable POSIX shell code even in a
  `-y.sh` file.

## Skipping a test case

- Where a test case depends on something the environment may not provide, probe
  for it and set `skip` to a non-empty value instead of letting the case fail.
  A failure that is not the shell's fault costs more to diagnose than a skip.
- Wrap the test case in a subshell so the assignment does not leak into the
  cases that follow:

  ```sh
  (
  # The state keyword of ps is not specified by POSIX.
  [ "$(ps -o state= -p $$ 2>/dev/null)" ] || skip="true"

  test_o -e 0 'a test case that needs ps -o state'
  ...
  __IN__
  ...
  __OUT__
  )
  ```

- Probe for the result, not just the exit status, where a utility may accept an
  unknown request and quietly produce nothing.
- The same idiom covers a behavior that depends on the platform, on the user
  (running as root), or on a feature that is not implemented yet.

## External utilities

- A test case may use external utilities to observe what the shell did — `ps`
  to read a process state, `diff` to compare files, `mkfifo` to synchronize.
  Prefer POSIX-specified utilities and options; guard anything else with `skip`.
- Command substitution does not strip the padding some utilities add. Use the
  word-splitting idiom `$(echo $(...))` where the value is compared or matched.

## Timing and synchronization

- Every test should finish as quickly as it can. The goal is that on an
  infinitely fast CPU the whole suite takes zero seconds, so a test case must
  never wait for time to pass.
- Never rely on a bare `sleep` to let something happen. Make the shell itself
  provide the synchronization point.
- A job that suspends itself in the foreground gives one for free: the shell
  regains control only once the job has stopped, so what follows cannot race
  with it.

  ```sh
  sh -c 'kill -s STOP $$'
  ```

- Otherwise use a FIFO (`mkfifo sync`) or poll for a definite condition, as
  existing job control tests do.
- Where a state change is inherently asynchronous, arrange the test so that the
  comparison does not depend on when it happens — print the markers after the
  job has finished rather than around it.

## Leaving no process behind

- A test case must not outlive itself. When the testee exits, every process it
  started should be gone; a `sleep` that lingers after the suite has finished is
  a defect in the test even when the case passes.
- Do not leave a background job running at the end of a case. Kill it, and be
  aware that the shell forks twice for an asynchronous external utility: the
  asynchronous job is a subshell that forks again to exec the utility, so `$!`
  names the subshell and `kill $!` leaves the utility orphaned.
- With job control (`-m`), each job has its own process group, so a job ID
  reaches the whole job. An `EXIT` trap is the tidiest place for it because it
  does not disturb the exit status the case is checking:

  ```sh
  test_O -e 0 'a case that needs a background job' -m
  trap 'kill -s KILL %1' EXIT
  sleep 10&
  ...
  __IN__
  ```

- Without job control, `kill %1` would signal the shell's own process group.
  Use a job whose body is a single built-in instead, so the job is one process
  that `kill $!` fully covers. A `read` blocking on a FIFO waits without
  spawning anything:

  ```sh
  mkfifo fifo

  test_OE -e 0 'a case that signals a background job'
  read x <fifo &
  kill -s KILL $!
  wait $!
  :
  __IN__
  ```

- After adding a case that starts a process, run the suite and check with `ps`
  that nothing is left behind.

## Before you are done

- Run the tests with `cargo test -p yash-cli --test scripted_test`. The harness
  prints the log only for a failing case; the full log of each file is left in
  `target/tmp/<name>.log`, which is where a `SKIPPED` line shows up.
- Confirm the new test fails without the change it covers. Temporarily undo the
  behavior, check that the case fails and that it fails on the line you meant it
  to, and restore the change. A scripted test that passes either way is worse
  than none.
