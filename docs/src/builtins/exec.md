# Exec built-in

The **`exec`** built-in replaces the current shell process with an external
utility invoked by treating the specified operands as a command. Without
operands, the built-in makes redirections applied to it permanent in the
current shell process.

## Synopsis

```sh
exec [name [arguments...]]
```

## Description

When invoked with operands, the exec built-in replaces the currently
executing shell process with a new process image, regarding the operands as
command words to start the external utility. The first operand identifies
the utility, and the other operands are passed to the utility as
command-line arguments.

Without operands, the built-in does not start any utility. Instead, it makes
any redirections performed in the calling simple command permanent in the
current shell environment. (This is done even if there are operands, but the
effect can be observed only when the utility cannot be invoked and the shell
does not exit.)

## Options

POSIX defines no options for the exec built-in.

The following non-portable options are yet to be implemented:

- `--as`
- `--clear`
- `--cloexec`
- `--force`
- `--help`

## Operands

The operands are treated as a command to start an external utility.
If any operands are given, the first is the utility name, and the others are
its arguments.

If the utility name contains a slash character, the shell will treat it as a
path to the utility.
Otherwise, the shell will [search `$PATH`](../language/commands/simple.md#command-search) for the utility.

## Errors

If the *name* operand is given, the named utility cannot be invoked, and the shell is not [interactive](../interactive/index.html), the current shell process will exit with an error.

## Exit status

If the external utility is invoked successfully, it replaces the shell
executing the built-in, so there is no exit status of the built-in.
If the built-in fails to invoke the utility, the exit status will be 126.
If there is no utility matching the first operand, the exit status will be
127.

If no operands are given, the exit status will be 0.

## Examples

To make the current shell process run `echo`:

```shell
$ exec echo "Hello, World!"
Hello, World!
```

Note that the `echo` executed here is not the built-in, but the external utility found in the `$PATH`. The shell process is replaced by the `echo` process, so you don't return to the shell prompt after the command.

See [Persistent redirections](../language/redirections/index.html#persistent-redirections) for examples of using the exec built-in to make redirections permanent.
