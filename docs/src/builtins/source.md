# Source (.) built-in

The **`source`** (**`.`**) built-in reads and executes commands from a file.

## Synopsis

```sh
. file [arguments…]
```

```sh
source file [arguments…]
```

## Description

The `.` built-in reads and executes commands from the specified file in the
current [shell environment](../environment/index.html).

If the filename does not contain a slash, the shell searches the directories in the `PATH` [variable](../language/parameters/variables.md) for the file. The search is similar to [command search](../language/commands/simple.md#command-search), but the file does not need to be
executable; any readable file found first is used.

<!-- TODO
If there are any operands after the filename, they are assigned to the [positional parameters](../language/parameters/positional.md) (`$1`, `$2`, etc.) like in a [function](../language/functions.md) call. In this case, the script can define local variables that are removed when the script finishes. The positional parameters are restored to their previous values when the script finishes.

If there are no operands, the positional parameters are not changed and the script cannot declare local variables.
-->

## Options

None.

<!-- TODO: non-portable options -->

## Operands

The first operand ***file*** must be given and is the pathname of the file
to be executed. If it does not contain a slash, it is subject to the search
described above.

Any additional ***arguments*** are currently ignored. Future versions may support assigning these to the [positional parameters](../language/parameters/positional.md).
<!-- TODO: Any remaining ***arguments*** are passed to the executed file as positional parameters. -->

## Errors

It is an error if the file cannot be found or read.
During parsing and execution, any syntax error or runtime error may occur.

## Exit status

The exit status of the built-in is the exit status of the last command executed in the file. If there is no command in the file, the exit status is zero.

If the file cannot be found or read, the exit status is 1.
In case of a syntax error in the file, the exit status is 2.

## Examples

See [Reading and executing files](../dynamic_evaluation.md#reading-and-executing-files).

## Compatibility

The `.` built-in is specified in the POSIX standard. The built-in name `source` is a non-standard extension that is also available in some other shells.

POSIX defines no options for the `.` built-in, but previous versions of yash supported additional options, which are not yet implemented in yash-rs.

POSIX does not require the `.` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.

If the pathname of the file does not contain a slash and the file is not found in the command search, some shells may fall back to the file in the current working directory. This is a non-portable extension that is not specified in POSIX. The portable way to specify a file in the current working directory is to prefix the filename with `./` as in `. ./foo.sh`.

Setting the positional parameters with additional operands is a non-standard extension that is supported by some other shells. The behavior about the local variable context may differ in other shells.

Other implementations may return a different non-zero exit status for an error.

According to POSIX.1-2024, "An unrecoverable read error while reading from the file operand of the dot special built-in shall be treated as a special built-in utility error." This means that if you use the `.` built-in through the [`command` built-in](command.md) and an unrecoverable read error occurs, the shell should not exit immediately. However, yash-rs does not currently support this behavior: if an unrecoverable read error happens and the shell is not running [interactively](../interactive/index.html), yash-rs will always exit. See [Shell errors](../termination.md#shell-errors) for the conditions under which the shell should exit.
