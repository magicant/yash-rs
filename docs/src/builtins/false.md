# False built-in

The **`false`** built-in does nothing, unsuccessfully.

## Synopsis

```sh
false
```

## Description

The `false` built-in does nothing and returns a non-zero exit
status.

## Options

None.

## Operands

None.

## Errors

None.

In the future, the built-in may detect unexpected options or operands.

## Exit Status

1\.

## Examples

See [And-or lists](../language/commands/exit_status.md#and-or-lists) for examples of using `false` in and-or lists. The [examples of the `getopts` built-in](getopts.md#examples) also use `false` to indicate that an option is not specified.

## Compatibility

The `false` utility is specified by POSIX.1-2024.

POSIX allows `false` to return any non-zero exit status, but most implementations return 1.

Most implementations ignore any arguments, but some implementations may respond to them. For example, the GNU coreutils implementation accepts the `--help` and `--version` options. For maximum portability, avoid passing arguments to `false`.

POSIX lists both the OPTIONS and OPERANDS sections of `false` as "None.", which means a conforming implementation need not support any option or operand. (Since 3.3.5) When the [`portable` option](../environment/options.md#portable) is set, the built-in prints a warning if it is given any argument, including a lone `--`. The warning does not affect the exit status, and the argument is still ignored.
