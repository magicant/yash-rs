# Trap built-in

The **`trap`** built-in sets or prints [traps](../environment/traps.md).

## Synopsis

```sh
trap [action] condition…
```

```sh
trap [-p [condition…]]
```

## Description

The `trap` built-in can be used to either set or print traps. To set traps, pass an *action* and one or more *condition*s as operands. To print the currently configured traps, invoke the built-in with no operands or with the `-p` option.

### Setting traps

When setting traps, the built-in sets the *action* for each *condition* in the current [shell environment]. To set different actions for multiple conditions, invoke the built-in for each *action*.

### Printing traps

When the built-in is invoked with no operands, it prints the currently configured traps in the format `trap -- action condition` where *action* and *condition* are properly [quoted](../language/words/quoting.md) so that the output can be [read](eval.md) by the shell to restore the traps. By default, the built-in prints traps that have non-default actions. To print all traps, use the `-p` option with no operands.

When the `-p` option is used with one or more *condition*s, the built-in prints the traps for the specified *condition*s.

When a [subshell](../environment/index.html#subshells) is entered, traps with a custom action are reset to the default action. This behavior would make it impossible to save the current traps by using a [command substitution](../language/words/command_substitution.md) as in `traps=$(trap)`. To make this work, when the built-in is invoked in a subshell and no traps have been modified in the subshell, it prints the traps that were configured in the parent shell.

## Options

The **`-p`** (**`--print`**) option prints the traps configured in the [shell environment].

## Operands

An ***action*** specifies what to do when the trap condition is met. It may be one of the following:

- `-` (hyphen) resets the trap to the default action.
- An empty string ignores the trap.
- Any other string is treated as a command to execute.

The *action* may be omitted if the first *condition* is a non-negative decimal integer. In this case, the built-in resets the trap to the default action.

A ***condition*** specifies when the action is triggered. It may be one of the following:

- A symbolic name of a [signal](../environment/traps.md#what-are-signals) without the `SIG` prefix (e.g. `INT`, `QUIT`, `TERM`)
    - Signal names must be specified in uppercase. Lowercase names and the `SIG` prefix may be supported in the future.
- A positive decimal integer representing a signal number
- The number `0` or the symbolic name `EXIT` representing the [termination](../termination.md) of the main shell process
    - This condition is not triggered when the shell exits due to a signal.

## Errors

Traps cannot be set to `SIGKILL` or `SIGSTOP`.

Invalid *condition*s are reported with a non-zero exit status, but it is not a [shell error](../termination.md#shell-errors).

If a non-interactive shell inherits a signal disposition that ignores a signal, it does not permit modifying the trap for that signal. In yash-rs, this restriction is enforced silently and does not affect the exit status of the built-in.

## Exit status

Zero if successful, non-zero if an error is reported.

## Examples

Setting a trap and reacting to a self-sent signal:

```shell
$ trap 'echo "Caught SIGINT"; exit' INT
$ kill -s INT $$
Caught SIGINT
```

Ignoring a signal:

```shell
$ trap '' INT
$ kill -s INT $$
$ echo "The shell survived the INT signal."
The shell survived the INT signal.
```

Showing traps:

```shell
$ trap '' INT QUIT; trap 'echo "Caught SIGTERM"' TERM
$ trap
trap -- '' INT
trap -- '' QUIT
trap -- 'echo "Caught SIGTERM"' TERM
$ trap -p INT
trap -- '' INT
```

## Compatibility

The `trap` built-in is specified by POSIX.1-2024.

Portable scripts should specify signals in uppercase letters without the `SIG` prefix. Specifying signals by numbers is discouraged as signal numbers vary among systems.

The result of setting a trap to `SIGKILL` or `SIGSTOP` is undefined by POSIX.

The mechanism for the built-in to print traps configured in the parent shell may vary among shells. This implementation remembers the old traps internally when starting a subshell and prints them when the built-in is invoked in the subshell. POSIX allows another scheme: When starting a subshell, the shell checks if the subshell command contains only a single invocation of the `trap` built-in, in which case the shell skips resetting traps on the subshell entry so that the built-in can print the traps configured in the parent shell. The check may be done by a simple literal comparison, so you should not expect the shell to recognize complex expressions such as `cmd=trap; traps=$($cmd)`.

In other shells, the `EXIT` condition may be triggered when the shell is terminated by a signal.

[shell environment]: ../environment/index.html
