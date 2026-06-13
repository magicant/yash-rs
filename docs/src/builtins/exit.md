# Exit built-in

The **`exit`** built-in causes the currently executing shell to exit.

## Synopsis

```sh
exit [-f] [exit_status]
```

## Description

`exit exit_status` makes the shell exit from the currently executing
environment with the specified exit status.

The shell executes the [EXIT trap](../termination.md#exit-trap), if any, before exiting, except when the built-in is invoked in the trap itself.

(Since 3.2.0) In an [interactive shell](../interactive/index.html), if there are
[suspended jobs](../interactive/job_control.md), the built-in prints a warning
and refuses to exit. Use the `-f` option to override this protection and exit
immediately.

## Options

`-f`, `--force`
: Force exit even when there are suspended jobs. Without this option, the
  built-in prints a warning and returns a non-zero exit status if there are
  any suspended jobs in an interactive shell.

## Operands

The optional ***exit_status*** operand, if given, should be a non-negative
decimal integer and will be the exit status of the exiting shell process.

## Errors

If the *exit_status* operand is given but not a valid non-negative integer,
it is a syntax error.

This implementation treats an *exit_status* value greater than 2147483647 as
a syntax error.

If there are suspended jobs in an interactive shell and `-f` is not given,
the built-in prints a warning to standard error and returns exit status 2
without exiting.

## Exit status

The *exit_status* operand specifies the exit status of the exiting shell.

If the operand is not given, the shell exits with the current exit status
(`$?`). If the built-in is invoked in a trap, the exit status will be the
value of `$?` before entering the trap.

In case of an error, the exit status is 2.

If the exit status indicates a signal that caused the process of the last command to terminate, the shell terminates with the same signal. See [Exit status of the shell](../language/commands/exit_status.md#exit-status-of-the-shell) for details.

## Examples

To exit the shell with exit status 42:

<!-- markdownlint-disable MD014 -->
```shell
$ exit 42
```
<!-- markdownlint-enable MD014 -->

To force exit when there are suspended jobs:

```shell,no_run
$ sleep 100
^Z
[1] + Stopped                 sleep 100
$ exit
# There are stopped jobs. Type `exit -f` to exit anyway.
$ exit -f
```

## Compatibility

The `exit` built-in is specified by POSIX.1-2024.

The `-f`/`--force` option and the suspended-jobs protection are
non-portable extensions introduced in yash-rs 3.2.0.

In some shells, the `exit` built-in lacks support for the [`--` separator](index.html#separators).

The behavior is undefined in POSIX if *exit_status* is greater than 255.
The current implementation passes such a value as is in the result, but this
behavior may change in the future.
