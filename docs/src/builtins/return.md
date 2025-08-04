# Return built-in.

The **`return`** built-in quits the currently executing innermost function
or script.

## Synopsis

```sh
return [-n] [exit_status]
```

## Description

`return exit_status` makes the shell return from the currently executing
function or script with the specified exit status.

## Options

The **`-n`** (**`--no-return`**) option makes the built-in not actually quit
a function or script. This option will be helpful when you want to set the
exit status to an arbitrary value without any other side effect.

## Operands

The optional ***exit_status*** operand, if given, should be a non-negative
decimal integer and will be the exit status of the built-in.

## Errors

If the *exit_status* operand is given but not a valid non-negative integer,
it is a syntax error.

This implementation treats an *exit_status* value greater than 2147483647 as
a syntax error.

TODO: What if there is no function or script to return from?

## Exit status

The *exit_status* operand will be the exit status of the built-in.

If the operand is not given, the exit status will be the current exit status
(`$?`). If the built-in is invoked in a trap executed in a function or
script and the built-in returns from that function or script, the exit
status will be the value of `$?` before entering the trap.

In case of an error, the exit status is 2 ([`ExitStatus::ERROR`]).

## Compatibility

POSIX only requires the return built-in to quit a function or dot script.
The behavior for other kinds of scripts is a non-standard extension.

The `-n` (`--no-return`) option is a non-standard extension.

The behavior is unspecified in POSIX if *exit_status* is greater than 255.
The current implementation passes such a value as is in the result, but this
behavior may change in the future.
