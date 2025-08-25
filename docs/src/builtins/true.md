# True built-in

The **`true`** built-in command does nothing, successfully.

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

(TODO: In the future, the built-in may detect unexpected options or operands.)

## Exit Status

Zero.

## Examples

See [And-or lists](../language/commands/exit_status.md#and-or-lists) for examples of using `true` in and-or lists. The [examples of the `getopts` built-in](getopts.md#examples) also use `true` to indicate that an option is specified.

## Compatibility

Most implementations ignore any arguments, but some implementations respond to them. For example, the GNU coreutils implementation accepts the `--help` and `--version` options. For maximum portability, avoid passing arguments to `true`. To pass and ignore arguments, use the [`:` (colon) built-in](colon.md) instead.
