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

(TODO: In the future, the built-in may detect unexpected options or operands.)

## Exit Status

1\.

## Examples

See [And-or lists](../language/commands/exit_status.md#and-or-lists) for examples of using `false` in and-or lists. The [examples of the `getopts` built-in](getopts.md#examples) also use `false` to indicate that an option is not specified.

## Portability

POSIX allows the `false` built-in to return any non-zero exit status, but
most implementations return 1.

Most implementations ignore any arguments, but some implementations may respond to them. For example, the GNU coreutils implementation accepts the `--help` and `--version` options. For maximum portability, avoid passing arguments to `false`.
