# True built-in

The **`true`** built-in does nothing, successfully.

## Synopsis

```sh
true
```

## Description

The `true` built-in does nothing, successfully. It is useful as a placeholder when a command is required but no action is needed.

## Options

None.

## Operands

None.

## Errors

None.

In the future, the built-in may detect unexpected options or operands.

## Exit Status

Zero.

## Examples

See [And-or lists](../language/commands/exit_status.md#and-or-lists) for examples of using `true` in and-or lists. The [examples of the `getopts` built-in](getopts.md#examples) also use `true` to indicate that an option is specified.

## Compatibility

The `true` utility is specified by POSIX.1-2024.

Most implementations ignore any arguments, but some implementations respond to them. For example, the GNU coreutils implementation accepts the `--help` and `--version` options. For maximum portability, avoid passing arguments to `true`. To pass and ignore arguments, use the [`:` (colon) built-in](colon.md) instead.

POSIX lists both the OPTIONS and OPERANDS sections of `true` as "None.", which means a conforming implementation need not support any option or operand. (Since 3.3.5) When the [`portable` option](../environment/options.md#portable) is set, the built-in prints a warning if it is given any argument, including a lone `--`. The warning does not affect the exit status, and the argument is still ignored.
